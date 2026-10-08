//! The floating, click-through window that draws keycaps.
//!
//! Everything is a Core Animation layer. Layers live as long as their keycap
//! stays on screen, so a cap can slide over when a neighbour arrives, sink
//! while its key is held and spring back up on release.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSAppearanceNameAqua,
    NSAppearanceNameDarkAqua, NSApplication, NSAttributedStringNSStringDrawing, NSBackingStoreType,
    NSColor, NSFont, NSFontAttributeName, NSFontDescriptorSystemDesignRounded, NSFontWeight,
    NSFontWeightBold, NSFontWeightMedium, NSFontWeightSemibold, NSForegroundColorAttributeName,
    NSPanel, NSScreen, NSScreenSaverWindowLevel, NSView, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColor;
use objc2_foundation::{
    NSArray, NSAttributedString, NSDictionary, NSMutableAttributedString, NSNumber, NSString,
    NSTimer,
};
use objc2_quartz_core::{
    CABasicAnimation, CAGradientLayer, CALayer, CAMediaTiming, CAMediaTimingFunction,
    CASpringAnimation, CATextLayer, CATransaction, kCAAlignmentCenter, kCAAlignmentLeft,
    kCAAlignmentRight, kCACornerCurveContinuous,
};

use crate::config::{Config, Position, Theme, parse_color};
use crate::display::{Cap, Frame, ItemKind};
use crate::theme::{KeyColors, Palette, Rgba};

/// Proportions of a keycap, all derived from its height.
struct Metrics {
    /// Keycap height.
    h: f64,
    /// Tray padding around the keys.
    pad: f64,
    /// Between keys of one combo.
    gap: f64,
    /// Between combos and text.
    item_gap: f64,
    radius: f64,
    /// How much of the keycap's side shows below its face.
    depth: f64,
    /// How far the face sinks when pressed.
    travel: f64,
    /// Side and top margins of the face within the cap.
    inset: f64,
    top: f64,
    /// Room around the tray for its shadow and the badge.
    margin: f64,
}

impl Metrics {
    fn new(h: f64) -> Metrics {
        let h = h.clamp(16.0, 400.0);
        Metrics {
            h,
            pad: 0.2 * h,
            gap: 0.1 * h,
            item_gap: 0.26 * h,
            radius: 0.2 * h,
            depth: 0.13 * h,
            travel: 0.08 * h,
            inset: 0.055 * h,
            top: 0.03 * h,
            margin: 0.6 * h,
        }
    }

    fn tray_radius(&self) -> f64 {
        // Concentric with the keycaps.
        self.radius + self.pad
    }
}

struct CapNode {
    skirt: Retained<CALayer>,
    face: Retained<CALayer>,
    sheen: Retained<CAGradientLayer>,
    legend: Retained<CATextLayer>,
    name: Retained<CATextLayer>,
    held: bool,
}

struct BadgeNode {
    pill: Retained<CALayer>,
    label: Retained<CATextLayer>,
    count: String,
}

#[derive(Default)]
struct Scene {
    caps: HashMap<(u64, String), CapNode>,
    texts: HashMap<u64, Retained<CATextLayer>>,
    badges: HashMap<u64, BadgeNode>,
    /// Nothing is on screen, so the next frame appears without sliding in
    /// from the previous one's layout.
    fresh: bool,
    panel_frame: Option<CGRect>,
}

pub struct Overlay {
    mtm: MainThreadMarker,
    panel: Retained<NSPanel>,
    tray: Retained<CALayer>,
    config: RefCell<Config>,
    scene: RefCell<Scene>,
    /// Bumped on every show, so stale hide timers know to do nothing.
    generation: Cell<u64>,
    visible: Cell<bool>,
}

impl Overlay {
    pub fn new(mtm: MainThreadMarker, config: &Config) -> Rc<Self> {
        let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1.0, 1.0));
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        let panel: Retained<NSPanel> = unsafe {
            msg_send![
                NSPanel::alloc(mtm),
                initWithContentRect: rect,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false,
            ]
        };
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setOpaque(false);
        panel.setHasShadow(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setIgnoresMouseEvents(true);
        panel.setLevel(NSScreenSaverWindowLevel);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );

        // A layer-hosting view: we own the layer tree, AppKit leaves it alone.
        let root = CALayer::new();
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect);
        view.setLayer(Some(&root));
        view.setWantsLayer(true);
        panel.setContentView(Some(&view));

        let tray = CALayer::new();
        tray.setCornerCurve(unsafe { kCACornerCurveContinuous });
        tray.setBorderWidth(1.0);
        root.addSublayer(&tray);

        Rc::new(Overlay {
            mtm,
            panel,
            tray,
            config: RefCell::new(config.clone()),
            scene: RefCell::new(Scene {
                fresh: true,
                ..Scene::default()
            }),
            generation: Cell::new(0),
            visible: Cell::new(false),
        })
    }

    pub fn apply_config(&self, config: &Config) {
        *self.config.borrow_mut() = config.clone();
        self.hide();
    }

    pub fn show(self: &Rc<Self>, frame: &Frame) {
        self.render(frame);

        let generation = self.generation.get() + 1;
        self.generation.set(generation);
        if !self.visible.replace(true) {
            self.panel.setAlphaValue(0.0);
            self.panel.orderFrontRegardless();
        }
        // Animating (rather than setting) the alpha also cancels a fade-out
        // that is still in flight.
        self.animate_alpha(1.0, 0.1, None);

        let hide_after = self.config.borrow().hide_after_ms as f64 / 1000.0;
        let this = Rc::clone(self);
        let fire = RcBlock::new(move |_: NonNull<NSTimer>| {
            if this.generation.get() == generation {
                this.fade_out(generation);
            }
        });
        // SAFETY: the timer is scheduled on, and fires on, the main run loop,
        // so the non-`Send` captures never leave the main thread.
        unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(hide_after, false, &fire) };
    }

    pub fn hide(&self) {
        self.generation.set(self.generation.get() + 1);
        self.visible.set(false);
        self.panel.orderOut(None);

        let mut scene = self.scene.borrow_mut();
        for node in scene.caps.values() {
            node.skirt.removeFromSuperlayer();
        }
        for layer in scene.texts.values() {
            layer.removeFromSuperlayer();
        }
        for badge in scene.badges.values() {
            badge.pill.removeFromSuperlayer();
        }
        *scene = Scene {
            fresh: true,
            panel_frame: scene.panel_frame,
            ..Scene::default()
        };
    }

    fn fade_out(self: &Rc<Self>, generation: u64) {
        let fade = self.config.borrow().fade_ms as f64 / 1000.0;
        let this = Rc::clone(self);
        let done = RcBlock::new(move || {
            if this.generation.get() == generation {
                this.hide();
            }
        });
        self.animate_alpha(0.0, fade, Some(done));
    }

    fn animate_alpha(&self, alpha: f64, duration: f64, done: Option<RcBlock<dyn Fn()>>) {
        let panel = &self.panel;
        let changes = RcBlock::new(move |ctx: NonNull<NSAnimationContext>| {
            unsafe { ctx.as_ref() }.setDuration(duration);
            panel.animator().setAlphaValue(alpha);
        });
        NSAnimationContext::runAnimationGroup_completionHandler(&changes, done.as_deref());
    }

    fn is_dark(&self, theme: Theme) -> bool {
        match theme {
            Theme::Dark => true,
            Theme::Light => false,
            Theme::Auto => {
                let names = unsafe {
                    NSArray::from_slice(&[NSAppearanceNameAqua, NSAppearanceNameDarkAqua])
                };
                NSApplication::sharedApplication(self.mtm)
                    .effectiveAppearance()
                    .bestMatchFromAppearancesWithNames(&names)
                    .is_some_and(|n| &*n == unsafe { NSAppearanceNameDarkAqua })
            }
        }
    }

    fn render(&self, frame: &Frame) {
        let config = self.config.borrow();
        let m = Metrics::new(config.style.size);
        let accent = parse_color(&config.style.accent_color).unwrap_or_else(|| {
            eprintln!("kave: invalid accent_color {:?}", config.style.accent_color);
            [1.0, 0.42, 0.24, 1.0]
        });
        let pal = Palette::new(self.is_dark(config.style.theme), accent);
        let Some(screen) = NSScreen::mainScreen(self.mtm) else {
            return;
        };
        let scale = screen.backingScaleFactor();
        let fonts = Fonts::new(&config.style.font, &m);
        let mut scene = self.scene.borrow_mut();

        // Lay everything out on one line, left to right.
        enum Placed<'a> {
            Cap {
                key: (u64, String),
                cap: &'a Cap,
                x: f64,
                w: f64,
            },
            Text {
                id: u64,
                text: Retained<NSAttributedString>,
                x: f64,
                w: f64,
                h: f64,
            },
            Badge {
                id: u64,
                label: Retained<NSAttributedString>,
                x: f64,
                w: f64,
            },
        }
        let mut placed = Vec::new();
        let mut x = 0.0;
        for (i, item) in frame.items.iter().enumerate() {
            if i > 0 {
                x += m.item_gap;
            }
            match &item.kind {
                ItemKind::Combo { caps, count } => {
                    for (j, cap) in caps.iter().enumerate() {
                        if j > 0 {
                            x += m.gap;
                        }
                        let w = cap_width(cap, &m, &fonts);
                        placed.push(Placed::Cap {
                            key: (item.id, cap.id.clone()),
                            cap,
                            x,
                            w,
                        });
                        x += w;
                    }
                    if *count > 1 {
                        let label = text_attr(&format!("×{count}"), &fonts.badge, pal.badge_text);
                        let w = (label.size().width + 0.3 * m.h)
                            .max(badge_height(&m))
                            .round();
                        x += m.gap;
                        placed.push(Placed::Badge {
                            id: item.id,
                            label,
                            x,
                            w,
                        });
                        x += w;
                    }
                }
                ItemKind::Text { text, clipped } => {
                    let text = typed_text(text, *clipped, &fonts.text, &pal);
                    let size = text.size();
                    let w = size.width.ceil();
                    placed.push(Placed::Text {
                        id: item.id,
                        text,
                        x,
                        w,
                        h: size.height.ceil(),
                    });
                    x += w;
                }
            }
        }

        let tray_size = CGSize::new(x + 2.0 * m.pad, m.h + 2.0 * m.pad);
        let area = screen.visibleFrame();
        let panel_size = CGSize::new(area.size.width, tray_size.height + 2.0 * m.margin);
        let y = match config.position {
            Position::Bottom => area.origin.y + config.margin - m.margin,
            Position::Top => {
                area.origin.y + area.size.height - config.margin - panel_size.height + m.margin
            }
            Position::Center => area.origin.y + (area.size.height - panel_size.height) / 2.0,
        };
        let panel_frame = CGRect::new(CGPoint::new(area.origin.x, y), panel_size);
        if scene.panel_frame != Some(panel_frame) {
            self.panel.setFrame_display(panel_frame, true);
            scene.panel_frame = Some(panel_frame);
        }

        CATransaction::begin();
        CATransaction::setAnimationDuration(0.24);
        CATransaction::setAnimationTimingFunction(Some(&ease_out()));
        CATransaction::setDisableActions(scene.fresh);
        scene.fresh = false;

        let tray = &self.tray;
        without_animation(|| {
            let show_tray = config.style.tray;
            tray.setBackgroundColor(show_tray.then(|| cg(pal.tray)).as_deref());
            tray.setBorderColor(show_tray.then(|| cg(pal.tray_border)).as_deref());
            tray.setCornerRadius(m.tray_radius());
            tray.setShadowColor(Some(&cg(pal.shadow)));
            tray.setShadowOpacity(if show_tray { 0.6 } else { 0.0 });
            tray.setShadowRadius(0.35 * m.h);
            tray.setShadowOffset(CGSize::new(0.0, -0.12 * m.h));
        });
        tray.setFrame(CGRect::new(
            CGPoint::new(
                ((panel_size.width - tray_size.width) / 2.0).round(),
                m.margin,
            ),
            tray_size,
        ));

        let mut seen_caps = HashSet::new();
        let mut seen_texts = HashSet::new();
        let mut seen_badges = HashSet::new();
        for p in &placed {
            match p {
                Placed::Cap { key, cap, x, w } => {
                    seen_caps.insert(key.clone());
                    let rect = CGRect::new(CGPoint::new(m.pad + x, m.pad), CGSize::new(*w, m.h));
                    let colors = if cap.accent {
                        &pal.accent_keys
                    } else {
                        &pal.keys
                    };
                    match scene.caps.get_mut(key) {
                        Some(node) => {
                            node.skirt.setFrame(rect);
                            if node.held != cap.held {
                                node.held = cap.held;
                                press(node, cap.held, &m);
                            }
                            style_cap(node, cap, colors, &m, &fonts, scale);
                        }
                        None => {
                            let node = new_cap(tray, rect, cap, colors, &m, &fonts, scale);
                            pop_in(&node.skirt, &m);
                            scene.caps.insert(key.clone(), node);
                        }
                    }
                }
                Placed::Text { id, text, x, w, h } => {
                    seen_texts.insert(*id);
                    let rect = CGRect::new(
                        CGPoint::new(m.pad + x, m.pad + ((m.h - h) / 2.0).round()),
                        CGSize::new(*w, *h),
                    );
                    let layer = scene.texts.entry(*id).or_insert_with(|| {
                        let layer = text_layer(scale);
                        without_animation(|| layer.setFrame(rect));
                        tray.addSublayer(&layer);
                        layer
                    });
                    without_animation(|| {
                        set_text(layer, text);
                        let mut r = layer.frame();
                        r.size = rect.size;
                        layer.setFrame(r);
                    });
                    layer.setFrame(rect);
                }
                Placed::Badge { id, label, x, w } => {
                    seen_badges.insert(*id);
                    let count = label.string().to_string();
                    let bh = badge_height(&m);
                    // Level with the keycap faces, not the whole cap.
                    let face_mid = m.pad + m.depth + (m.h - m.depth - m.top) / 2.0;
                    let rect = CGRect::new(
                        CGPoint::new(m.pad + x, (face_mid - bh / 2.0).round()),
                        CGSize::new(*w, bh),
                    );
                    let is_new = !scene.badges.contains_key(id);
                    let badge = scene.badges.entry(*id).or_insert_with(|| {
                        let pill = CALayer::new();
                        pill.setCornerCurve(unsafe { kCACornerCurveContinuous });
                        let label = text_layer(scale);
                        pill.addSublayer(&label);
                        without_animation(|| pill.setFrame(rect));
                        tray.addSublayer(&pill);
                        BadgeNode {
                            pill,
                            label,
                            count: String::new(),
                        }
                    });
                    without_animation(|| {
                        let pill = &badge.pill;
                        pill.setBackgroundColor(Some(&cg(pal.badge)));
                        pill.setCornerRadius(bh / 2.0);
                        pill.setShadowColor(Some(&cg(pal.shadow)));
                        pill.setShadowOpacity(0.4);
                        pill.setShadowRadius(0.06 * m.h);
                        pill.setShadowOffset(CGSize::new(0.0, -0.03 * m.h));
                        let lh = label.size().height.ceil();
                        set_text(&badge.label, label);
                        badge.label.setFrame(CGRect::new(
                            CGPoint::new(0.0, ((bh - lh) / 2.0).round()),
                            CGSize::new(*w, lh),
                        ));
                        badge.label.setAlignmentMode(unsafe { kCAAlignmentCenter });
                    });
                    badge.pill.setFrame(rect);
                    if badge.count != count {
                        badge.count = count;
                        spring(
                            &badge.pill,
                            "transform.scale",
                            if is_new { 0.4 } else { 1.3 },
                            1.0,
                        );
                    }
                }
            }
        }

        scene.caps.retain(|k, node| {
            let keep = seen_caps.contains(k);
            if !keep {
                node.skirt.removeFromSuperlayer();
            }
            keep
        });
        scene.texts.retain(|k, layer| {
            let keep = seen_texts.contains(k);
            if !keep {
                layer.removeFromSuperlayer();
            }
            keep
        });
        scene.badges.retain(|k, badge| {
            let keep = seen_badges.contains(k);
            if !keep {
                badge.pill.removeFromSuperlayer();
            }
            keep
        });
        CATransaction::commit();
    }
}

struct Fonts {
    legend_big: Retained<NSFont>,
    legend_mid: Retained<NSFont>,
    legend_small: Retained<NSFont>,
    /// Corner legend on keys that also have a name.
    corner: Retained<NSFont>,
    name: Retained<NSFont>,
    /// Keys labelled only with a word, like esc and space.
    word: Retained<NSFont>,
    text: Retained<NSFont>,
    badge: Retained<NSFont>,
}

impl Fonts {
    fn new(family: &str, m: &Metrics) -> Fonts {
        let font = |size: f64, weight: NSFontWeight| {
            let size = size.round();
            Some(family)
                .filter(|f| !f.is_empty())
                .and_then(|f| NSFont::fontWithName_size(&NSString::from_str(f), size))
                .unwrap_or_else(|| rounded(size, weight))
        };
        let (medium, semibold, bold) =
            unsafe { (NSFontWeightMedium, NSFontWeightSemibold, NSFontWeightBold) };
        Fonts {
            legend_big: font(0.4 * m.h, medium),
            legend_mid: font(0.3 * m.h, medium),
            legend_small: font(0.22 * m.h, semibold),
            corner: font(0.27 * m.h, medium),
            name: font(0.155 * m.h, semibold),
            word: font(0.2 * m.h, semibold),
            text: font(0.44 * m.h, medium),
            badge: font(0.22 * m.h, bold),
        }
    }

    fn legend(&self, legend: &str) -> &NSFont {
        match legend.chars().count() {
            0 | 1 => &self.legend_big,
            2 | 3 => &self.legend_mid,
            _ => &self.legend_small,
        }
    }
}

fn rounded(size: f64, weight: NSFontWeight) -> Retained<NSFont> {
    let system = NSFont::systemFontOfSize_weight(size, weight);
    system
        .fontDescriptor()
        .fontDescriptorWithDesign(unsafe { NSFontDescriptorSystemDesignRounded })
        .and_then(|d| NSFont::fontWithDescriptor_size(&d, size))
        .unwrap_or(system)
}

/// Keys like return and delete sit on the right of a keyboard, so their name
/// is printed on the right, as on Apple keyboards.
fn name_on_right(cap: &Cap) -> bool {
    matches!(cap.id.as_str(), "return" | "enter" | "backspace" | "delete")
}

fn cap_width(cap: &Cap, m: &Metrics, fonts: &Fonts) -> f64 {
    let units = cap.width.max(1.0);
    let grid = units * m.h + (units - 1.0) * m.gap;
    let content = match &cap.name {
        Some(name) if cap.legend.is_empty() => {
            text_attr(name, &fonts.word, [0.0; 4]).size().width + 2.0 * (0.12 * m.h + m.inset)
        }
        Some(name) => {
            let name_w = text_attr(name, &fonts.name, [0.0; 4]).size().width;
            let corner_w = text_attr(&cap.legend, &fonts.corner, [0.0; 4]).size().width;
            name_w.max(corner_w) + 2.0 * (0.12 * m.h + m.inset)
        }
        None => {
            let legend = text_attr(&cap.legend, fonts.legend(&cap.legend), [0.0; 4]);
            legend.size().width + 0.4 * m.h
        }
    };
    grid.max(content).round()
}

fn badge_height(m: &Metrics) -> f64 {
    (0.4 * m.h).round()
}

fn face_rect(cap: CGSize, held: bool, m: &Metrics) -> CGRect {
    let y = if held { m.depth - m.travel } else { m.depth };
    CGRect::new(
        CGPoint::new(m.inset, y),
        CGSize::new(cap.width - 2.0 * m.inset, cap.height - m.depth - m.top),
    )
}

fn new_cap(
    tray: &CALayer,
    rect: CGRect,
    cap: &Cap,
    colors: &KeyColors,
    m: &Metrics,
    fonts: &Fonts,
    scale: f64,
) -> CapNode {
    let skirt = CALayer::new();
    let face = CALayer::new();
    let sheen = CAGradientLayer::new();
    let legend = text_layer(scale);
    let name = text_layer(scale);
    for layer in [&*skirt, &*face, &**sheen] {
        layer.setCornerCurve(unsafe { kCACornerCurveContinuous });
    }
    // Light from above: fades out by the middle of the face.
    sheen.setStartPoint(CGPoint::new(0.5, 1.0));
    sheen.setEndPoint(CGPoint::new(0.5, 0.4));
    face.addSublayer(&sheen);
    face.addSublayer(&legend);
    face.addSublayer(&name);
    skirt.addSublayer(&face);

    let node = CapNode {
        skirt,
        face,
        sheen,
        legend,
        name,
        held: cap.held,
    };
    without_animation(|| {
        node.skirt.setFrame(rect);
        style_cap(&node, cap, colors, m, fonts, scale);
    });
    tray.addSublayer(&node.skirt);
    node
}

/// Colors, legends and face geometry. Never animated: only movement is.
fn style_cap(
    node: &CapNode,
    cap: &Cap,
    colors: &KeyColors,
    m: &Metrics,
    fonts: &Fonts,
    scale: f64,
) {
    without_animation(|| {
        let skirt = &node.skirt;
        let size = skirt.bounds().size;
        skirt.setCornerRadius(m.radius);
        skirt.setBackgroundColor(Some(&cg(colors.skirt)));
        skirt.setShadowColor(Some(&cg([0.0, 0.0, 0.0, 1.0])));
        skirt.setShadowOpacity(0.35);
        skirt.setShadowRadius(0.05 * m.h);
        skirt.setShadowOffset(CGSize::new(0.0, -0.035 * m.h));

        let face = &node.face;
        if face.animationKeys().is_none() {
            face.setFrame(face_rect(size, node.held, m));
        }
        face.setCornerRadius(m.radius - 0.5 * m.inset);
        let fill = if node.held {
            colors.face_pressed
        } else {
            colors.face
        };
        face.setBackgroundColor(Some(&cg(fill)));
        face.setBorderWidth(1.0);
        face.setBorderColor(Some(&cg(colors.bevel)));

        let fw = size.width - 2.0 * m.inset;
        let fh = size.height - m.depth - m.top;
        let sheen = &node.sheen;
        sheen.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(fw, fh)));
        sheen.setCornerRadius(m.radius - 0.5 * m.inset);
        let [r, g, b, _] = colors.sheen;
        set_colors(sheen, &[colors.sheen, [r, g, b, 0.0]]);
        let lpad = 0.11 * m.h;
        node.legend.setContentsScale(scale);
        node.name.setContentsScale(scale);
        match &cap.name {
            Some(name) if !cap.legend.is_empty() => {
                let corner = text_attr(&cap.legend, &fonts.corner, colors.legend);
                let lh = corner.size().height.ceil();
                set_text(&node.legend, &corner);
                node.legend.setAlignmentMode(unsafe { kCAAlignmentRight });
                node.legend.setFrame(CGRect::new(
                    CGPoint::new(lpad, (fh - 0.6 * lpad - lh).round()),
                    CGSize::new(fw - 2.0 * lpad, lh),
                ));
                let word = text_attr(name, &fonts.name, colors.name);
                let nh = word.size().height.ceil();
                set_text(&node.name, &word);
                node.name.setAlignmentMode(if name_on_right(cap) {
                    unsafe { kCAAlignmentRight }
                } else {
                    unsafe { kCAAlignmentLeft }
                });
                node.name.setFrame(CGRect::new(
                    CGPoint::new(lpad, (0.55 * lpad).round()),
                    CGSize::new(fw - 2.0 * lpad, nh),
                ));
            }
            Some(name) => {
                // The space bar and friends: just the word, centered.
                let word = text_attr(name, &fonts.word, colors.legend);
                let nh = word.size().height.ceil();
                set_text(&node.name, &word);
                node.name.setAlignmentMode(unsafe { kCAAlignmentCenter });
                node.name.setFrame(CGRect::new(
                    CGPoint::new(0.0, ((fh - nh) / 2.0).round()),
                    CGSize::new(fw, nh),
                ));
                set_text(&node.legend, &text_attr("", &fonts.corner, colors.legend));
            }
            None => {
                let legend = text_attr(&cap.legend, fonts.legend(&cap.legend), colors.legend);
                let lh = legend.size().height.ceil();
                set_text(&node.legend, &legend);
                node.legend.setAlignmentMode(unsafe { kCAAlignmentCenter });
                node.legend.setFrame(CGRect::new(
                    CGPoint::new(0.0, ((fh - lh) / 2.0).round()),
                    CGSize::new(fw, lh),
                ));
                set_text(&node.name, &text_attr("", &fonts.name, colors.name));
            }
        }
    });
}

/// Sink the face into the cap, or let it spring back up.
fn press(node: &CapNode, held: bool, m: &Metrics) {
    let size = node.skirt.bounds().size;
    let to = face_rect(size, held, m);
    let from_y = node.face.position().y;
    let to_y = to.origin.y + to.size.height / 2.0;
    without_animation(|| node.face.setFrame(to));
    if held {
        let anim = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("position.y")));
        unsafe {
            anim.setFromValue(Some(&NSNumber::new_f64(from_y)));
            anim.setToValue(Some(&NSNumber::new_f64(to_y)));
        }
        anim.setDuration(0.05);
        anim.setTimingFunction(Some(&ease_out()));
        node.face
            .addAnimation_forKey(&anim, Some(&NSString::from_str("press")));
    } else {
        spring(&node.face, "position.y", from_y, to_y);
    }
}

/// New keycaps drop into place.
fn pop_in(layer: &CALayer, m: &Metrics) {
    spring(layer, "transform.scale", 0.6, 1.0);
    spring(layer, "transform.translation.y", 0.35 * m.h, 0.0);
    let fade = CABasicAnimation::animationWithKeyPath(Some(&NSString::from_str("opacity")));
    unsafe {
        fade.setFromValue(Some(&NSNumber::new_f64(0.0)));
        fade.setToValue(Some(&NSNumber::new_f64(1.0)));
    }
    fade.setDuration(0.12);
    layer.addAnimation_forKey(&fade, Some(&NSString::from_str("fade")));
}

fn spring(layer: &CALayer, key_path: &str, from: f64, to: f64) {
    let anim = CASpringAnimation::animationWithKeyPath(Some(&NSString::from_str(key_path)));
    unsafe {
        anim.setFromValue(Some(&NSNumber::new_f64(from)));
        anim.setToValue(Some(&NSNumber::new_f64(to)));
    }
    anim.setMass(1.0);
    anim.setStiffness(420.0);
    anim.setDamping(18.0);
    anim.setDuration(anim.settlingDuration());
    layer.addAnimation_forKey(&anim, Some(&NSString::from_str(key_path)));
}

fn ease_out() -> Retained<CAMediaTimingFunction> {
    CAMediaTimingFunction::functionWithControlPoints(0.2, 0.9, 0.25, 1.0)
}

fn without_animation(f: impl FnOnce()) {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    f();
    CATransaction::commit();
}

fn text_layer(scale: f64) -> Retained<CATextLayer> {
    let layer = CATextLayer::new();
    layer.setContentsScale(scale);
    layer.setWrapped(false);
    layer
}

fn set_text(layer: &CATextLayer, text: &NSAttributedString) {
    let text: &AnyObject = text;
    unsafe { layer.setString(Some(text)) };
}

fn text_attr(text: &str, font: &NSFont, color: Rgba) -> Retained<NSAttributedString> {
    let [r, g, b, a] = color;
    let color = NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, a);
    let values: [&AnyObject; 2] = [font, &color];
    let keys = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let attrs = NSDictionary::from_slices(&keys, &values);
    unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(&attrs),
        )
    }
}

/// Typed text, with spaces made visible as dim `␣`.
fn typed_text(
    text: &str,
    clipped: bool,
    font: &NSFont,
    pal: &Palette,
) -> Retained<NSAttributedString> {
    let out = NSMutableAttributedString::new();
    if clipped {
        out.appendAttributedString(&text_attr("…", font, pal.text_dim));
    }
    for (i, part) in text.split(' ').enumerate() {
        if i > 0 {
            out.appendAttributedString(&text_attr("␣", font, pal.text_dim));
        }
        if !part.is_empty() {
            out.appendAttributedString(&text_attr(part, font, pal.text));
        }
    }
    Retained::into_super(out)
}

fn set_colors(layer: &CAGradientLayer, colors: &[Rgba]) {
    let colors: Vec<_> = colors.iter().map(|&c| cg(c)).collect();
    // SAFETY: CGColor is toll-free bridged to an Objective-C object.
    let objects: Vec<&AnyObject> = colors
        .iter()
        .map(|c| unsafe { &*(&**c as *const CGColor).cast::<AnyObject>() })
        .collect();
    let array = NSArray::from_slice(&objects);
    unsafe { layer.setColors(Some(&array)) };
}

fn cg([r, g, b, a]: Rgba) -> CFRetained<CGColor> {
    CGColor::new_srgb(r, g, b, a)
}
