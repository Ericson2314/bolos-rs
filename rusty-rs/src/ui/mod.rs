mod bolos;

use core::marker::PhantomData;
use core::convert::Into;
use pic::Pic;
use seproxyhal::Channel;
use seproxyhal::event::{Event, ButtonPushEvent};
use seproxyhal::status::{
    ScreenDisplayStatus, ScreenDisplayStatusTypeId, ScreenDisplayShapeStatus,
    ScreenDisplayTextStatus, ScreenDisplaySystemIconStatus, ScreenDisplayCustomIconStatus,
};

struct ButtonActionMap<A> {
    left: Option<A>,
    right: Option<A>,
    both: Option<A>,
}

impl<A> Default for ButtonActionMap<A> {
    fn default() -> Self {
        Self{
            left: None,
            right: None,
            both: None,
        }
    }
}

pub enum ButtonAction<A: Copy> {
    Map{
        left: Option<A>,
        right: Option<A>,
        both: Option<A>,
    },
    ForAll(A),
    None,
}

impl<A> Into<ButtonActionMap<A>> for ButtonAction<A>
    where A: Copy
{
    fn into(self) -> ButtonActionMap<A> {
        match self {
            ButtonAction::Map{ left: l, right: r, both: b } => ButtonActionMap{
                left: l,
                right: r,
                both: b,
            },
            ButtonAction::ForAll(a) => ButtonActionMap{
                left: Some(a),
                right: Some(a),
                both: Some(a),
            },
            ButtonAction::None => Default::default(),
        }
    }
}

pub struct Middleware<A, D> {
    current_view_index: usize,
    button_actions: ButtonActionMap<A>,
    button_bits: u8,
    button_timer: usize,
    phantom_delegate: PhantomData<D>,
}

impl<A, D> Middleware<A, D>
    where A: Copy,
          D: Delegate<Action=A>,
{
    pub fn new() -> Self {
        Self{
            current_view_index: 0,
            button_actions: Default::default(),
            button_bits: 0,
            button_timer: 0,
            phantom_delegate: PhantomData,
        }
    }

    fn reset_for_redraw(&mut self) {
        let this = self.pic();
        this.current_view_index = 0;
        this.button_actions = Default::default();
    }

    fn send_next_view(&mut self, ch: Channel, delegate: &mut D) -> Option<Channel>
    {
        let this = self.pic();

        // Coordinate our rendering with the system UI
        match bolos::event() {
            bolos::Response::Redraw => {
                this.reset_for_redraw();
            },
            bolos::Response::Ignore | bolos::Response::Continue => {
                return Some(ch);
            },
            _ => {},
        }

        // See if there's another view to render
        let mut ctrl = Controller::new(this.current_view_index);
        delegate.prepare_ui(&mut ctrl);
        if let Some(ref view) = ctrl.target_view {
            this.button_actions = ctrl.button_actions;

            match view {
                View::LabelLine(LabelLineView{
                    scroll: ScrollMode::Once{ speed, delay, finished_action },
                    text,
                    ..
                }) => {
                    // TODO: Calculate the time it would take for that text to scroll and register the action
                    let _ = (speed, delay, finished_action, text);
                },

                _ => {},
            }

            let status = view.to_display_status(0).into();
            ch.send_status(status);

            None
        } else {
            Some(ch)
        }
    }

    fn process_button_presses(&mut self, mut button_bits: u8, delegate: &mut D) {
        let this = self.pic();

        const KEY_REPEAT_THRESHOLD: usize = 8; // 800ms
        const KEY_REPEAT_DELAY: usize = 3; // 300ms
        const LEFT_BUTTON: u8 = 1 << 0;
        const RIGHT_BUTTON: u8 = 1 << 1;
        const BOTH_BUTTONS: u8 = LEFT_BUTTON | RIGHT_BUTTON;
        let is_released = button_bits == 0;

        if this.button_bits == button_bits {
            this.button_timer += 1; // once every ~100ms
        } else if !is_released {
            // Reset when the bits change
            this.button_timer = 0;
        }

        let (pressed_bits, repeating) = if is_released {
            button_bits = this.button_bits;
            this.button_bits = 0;
            this.button_timer = 0;
            (button_bits, false)
        } else if this.button_timer > KEY_REPEAT_THRESHOLD
            && this.button_timer % KEY_REPEAT_DELAY == 0 {
            (button_bits, true)
        } else {
            (0, false)
        };

        let action = match (pressed_bits, repeating) {
            (LEFT_BUTTON, _) => this.button_actions.left,
            (RIGHT_BUTTON, _) => this.button_actions.right,
            (BOTH_BUTTONS, false) => this.button_actions.both,
            _ => None,
        };

        if let Some(action) = action {
            delegate.process_action(action);
        }
    }

    pub fn process_event(&mut self, ch: Channel, delegate: &mut D) -> Option<Channel> {
        let this = self.pic();

        match ch.event {
            Event::DisplayProcessed(_) => {
                this.current_view_index += 1;
            },
            Event::ButtonPush(ButtonPushEvent{ flags }) => {
                this.process_button_presses(flags >> 1, delegate);
            },
            _ => {},
        }

        if delegate.should_redraw() {
            this.reset_for_redraw();
        }
        this.send_next_view(ch, delegate)
    }

    pub fn redraw_if_needed(&mut self, ch: Channel, delegate: &mut D) -> Option<Channel> {
        let this = self.pic();

        if delegate.should_redraw() {
            this.reset_for_redraw();
            this.send_next_view(ch, delegate)
        } else {
            Some(ch)
        }
    }
}

pub struct Controller<'a, A: Copy> {
    target_index: usize,
    current_index: usize,
    target_view: Option<View<'a, A>>,
    button_actions: ButtonActionMap<A>,
}

impl<'a, A> Controller<'a, A>
    where A: Copy
{
    fn new(target_index: usize) -> Self {
        Self{
            target_index: target_index,
            current_index: 0,
            target_view: None,
            button_actions: Default::default(),
        }
    }

    #[inline(always)]
    pub fn add_view<F>(&mut self, lazy_view: F)
        where F: FnOnce() -> View<'a, A>
    {
        let this = self.pic();
        if this.target_index == this.current_index {
            this.target_view = lazy_view().into();
        }
        this.current_index += 1;
    }

    pub fn set_button_actions(&mut self, actions: ButtonAction<A>) {
        let this = self.pic();
        this.button_actions = actions.into();
    }
}

#[derive(Copy, Clone)]
pub enum BasicAction {
    Previous,
    Next,
    Confirm,
}

pub trait Delegate {
    type Action: Copy;

    fn prepare_ui(&mut self, ctrl: &mut Controller<Self::Action>);
    fn should_redraw(&self) -> bool;
    fn process_action(&mut self, _action: Self::Action) {}
}

pub enum FillMode {
    NoFill,
    Fill,
    Outline,
}

impl FillMode {
    fn to_wire_format(&self) -> u8 {
        let this = self.pic();
        match this {
            &FillMode::NoFill => 0,
            &FillMode::Fill => 1,
            &FillMode::Outline => 2,
        }
    }
}

pub struct Color(u32);

impl Color {
    fn to_wire_format(&self) -> u32 {
        let this = self.pic();
        this.0
    }

    pub fn white() -> Self {
        Color(0xFFFFFF)
    }

    pub fn black() -> Self {
        Color(0x000000)
    }
}

pub struct Frame {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

impl Default for Frame {
    fn default() -> Self {
        Self{ x: 0, y: 0, width: 0, height: 0 }
    }
}

pub struct RectangleView {
    pub frame: Frame,
    pub stroke: u8,
    pub radius: u8,
    pub fill: FillMode,
    pub foreground: Color,
    pub background: Color,
}

impl RectangleView {
    fn to_display_status(&self, user_id: u8) -> ScreenDisplayStatus {
        let this = self.pic();

        ScreenDisplayShapeStatus{
            type_id: ScreenDisplayStatusTypeId::Rectangle,
            user_id: user_id,
            x: this.frame.x,
            y: this.frame.y,
            width: this.frame.width,
            height: this.frame.height,
            stroke: this.stroke,
            radius: this.radius,
            fill: this.fill.to_wire_format(),
            foreground_color: this.foreground.to_wire_format(),
            background_color: this.background.to_wire_format(),
        }.into()
    }
}

impl Default for RectangleView {
    fn default() -> Self {
        Self{
            frame: Default::default(),
            stroke: 0,
            radius: 0,
            fill: FillMode::NoFill,
            foreground: Color::black(),
            background: Color::white(),
        }
    }
}

impl<'a, A> Into<View<'a, A>> for RectangleView {
    fn into(self) -> View<'a, A> {
        View::Rectangle(self)
    }
}

pub enum SystemIcon {
    Check,
    Cross,
}

impl SystemIcon {
    fn to_wire_format(&self) -> u8 {
        let this = self.pic();
        match this {
            &SystemIcon::Check => 6,
            &SystemIcon::Cross => 7,
        }
    }
}

impl<'a> Into<Icon<'a>> for SystemIcon {
    fn into(self) -> Icon<'a> {
        Icon::System(self)
    }
}

pub struct CustomIcon<'a> {
    pub bits_per_pixel: u8,
    pub colors: &'a [u32],
    pub bitmap: &'a [u8],
}

impl<'a> Into<Icon<'a>> for CustomIcon<'a> {
    fn into(self) -> Icon<'a> {
        Icon::Custom(self)
    }
}

pub enum Icon<'a> {
    System(SystemIcon),
    Custom(CustomIcon<'a>),
}

pub struct IconView<'a> {
    pub frame: Frame,
    pub icon: Icon<'a>,
}

impl<'a> IconView<'a> {
    fn to_display_status(&self, user_id: u8) -> ScreenDisplayStatus {
        let this = self.pic();

        match this.icon {
            Icon::Custom(ref icon) => {
                ScreenDisplayCustomIconStatus{
                    user_id: user_id,
                    x: this.frame.x,
                    y: this.frame.y,
                    width: this.frame.width,
                    height: this.frame.height,
                    bits_per_pixel: icon.bits_per_pixel,
                    colors: icon.colors,
                    bitmap: icon.bitmap,
                }.into()
            },
            Icon::System(ref icon) => {
                ScreenDisplaySystemIconStatus{
                    user_id: user_id,
                    x: this.frame.x,
                    y: this.frame.y,
                    width: this.frame.width,
                    height: this.frame.height,
                    icon_id: icon.to_wire_format(),
                }.into()
            },
        }
    }
}

impl<'a> Default for IconView<'a> {
    fn default() -> Self {
        Self{
            frame: Default::default(),
            icon: CustomIcon{
                bits_per_pixel: 0,
                colors: &[],
                bitmap: &[],
            }.into(),
        }
    }
}

impl<'a, A> Into<View<'a, A>> for IconView<'a> {
    fn into(self) -> View<'a, A> {
        View::Icon(self)
    }
}

pub enum ScrollMode<A> {
    Disabled,
    Once{
        delay: u8,
        speed: u8,
        finished_action: Option<A>,
    },
    Infinite{
        delay: u8,
        speed: u8,
    },
}

impl<A> ScrollMode<A> {
    fn to_wire_format(&self) -> (u8, u8) {
        let this = self.pic();
        let scroll_delay;
        let scroll_speed;
        match this {
            &ScrollMode::Disabled => {
                scroll_delay = 0;
                scroll_speed = 0;
            },
            &ScrollMode::Once{ delay, speed, .. } => {
                scroll_delay = delay | 0x80;
                scroll_speed = speed;
            },
            &ScrollMode::Infinite{ delay, speed } => {
                scroll_delay = delay;
                scroll_speed = speed;
            },
        };
        (scroll_delay, scroll_speed)
    }
}

pub enum TextHorizontalAlignment {
    Left,
    Center,
    Right,
}

impl TextHorizontalAlignment {
    fn to_wire_format(&self) -> u16 {
        let this = self.pic();
        match this {
            &TextHorizontalAlignment::Left => 0x0000,
            &TextHorizontalAlignment::Center => 0x4000,
            &TextHorizontalAlignment::Right => 0x8000,
        }
    }
}

pub enum TextVerticalAlignment {
    Top,
    Middle,
    Bottom,
}

impl TextVerticalAlignment {
    fn to_wire_format(&self) -> u16 {
        let this = self.pic();
        match this {
            &TextVerticalAlignment::Top => 0x0000,
            &TextVerticalAlignment::Middle => 0x1000,
            &TextVerticalAlignment::Bottom => 0x2000,
        }
    }
}

pub enum TextFont {
    OpenSansLight16px,
    OpenSansRegular11px,
    OpenSansExtraBold11px,
}

impl TextFont {
    fn to_wire_format(&self) -> u16 {
        let this = self.pic();
        match this {
            &TextFont::OpenSansLight16px => 9,
            &TextFont::OpenSansRegular11px => 10,
            &TextFont::OpenSansExtraBold11px => 8,
        }
    }
}

pub struct LabelLineView<'a, A> {
    pub frame: Frame,
    pub font: TextFont,
    pub horizontal_alignment: TextHorizontalAlignment,
    pub vertical_alignment: TextVerticalAlignment,
    pub scroll: ScrollMode<A>,
    pub foreground: Color,
    pub background: Color,
    pub fill: FillMode,
    pub text: &'a str,
}

impl<'a, A> LabelLineView<'a, A> {
    fn to_display_status(&self, user_id: u8) -> ScreenDisplayStatus {
        let this = self.pic();

        let (scroll_delay, scroll_speed) = this.scroll.to_wire_format();
        let font_id = this.font.to_wire_format()
            | this.horizontal_alignment.to_wire_format()
            | this.vertical_alignment.to_wire_format();

        ScreenDisplayTextStatus{
            type_id: ScreenDisplayStatusTypeId::LabelLine,
            user_id: user_id,
            x: this.frame.x,
            y: this.frame.y,
            width: this.frame.width,
            height: this.frame.height,
            scroll_delay: scroll_delay,
            scroll_speed: scroll_speed,
            fill: this.fill.to_wire_format(),
            foreground_color: this.foreground.to_wire_format(),
            background_color: this.background.to_wire_format(),
            font_id: font_id,
            text: this.text,
        }.into()
    }
}

impl<'a, A> Default for LabelLineView<'a, A> {
    fn default() -> Self {
        Self{
            frame: Default::default(),
            font: TextFont::OpenSansRegular11px,
            horizontal_alignment: TextHorizontalAlignment::Left,
            vertical_alignment: TextVerticalAlignment::Top,
            scroll: ScrollMode::Disabled,
            foreground: Color::white(),
            background: Color::black(),
            fill: FillMode::NoFill,
            text: "",
        }
    }
}

impl<'a, A> Into<View<'a, A>> for LabelLineView<'a, A> {
    fn into(self) -> View<'a, A> {
        View::LabelLine(self)
    }
}

pub enum View<'a, A> {
    Rectangle(RectangleView),
    Icon(IconView<'a>),
    LabelLine(LabelLineView<'a, A>),
}

impl<'a, A> View<'a, A> {
    fn to_display_status(&self, user_id: u8) -> ScreenDisplayStatus {
        let this = self.pic();
        match this {
            &View::Rectangle(ref v) => v.to_display_status(user_id),
            &View::Icon(ref v) => v.to_display_status(user_id),
            &View::LabelLine(ref v) => v.to_display_status(user_id),
        }
    }
}