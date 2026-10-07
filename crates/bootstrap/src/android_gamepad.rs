//! Gamepads on Android. gilrs has no Android backend, and winit reads a
//! controller's sticks as touches, so the patched `android-activity` input
//! filter claims controller events before winit and they are fed to Bevy here
//! the way `bevy_gilrs` feeds them on desktop.
//!
//! Android sends nothing on disconnect through the input queue, so a pad stays
//! registered until the app restarts; it simply stops producing events.

use std::collections::HashMap;
use std::sync::Mutex;

use android_activity::input::{Axis, InputEvent, KeyAction};
use bevy::input::InputSystems;
use bevy::input::gamepad::{
    GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent,
    RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
};
use bevy::prelude::*;

/// `AINPUT_SOURCE_GAMEPAD` and `AINPUT_SOURCE_JOYSTICK`. A source is a bit
/// set, so a controller's keys arrive as gamepad | keyboard and its sticks as
/// joystick, and only the bits are compared.
const GAMEPAD_SOURCE_BITS: u32 = 0x0000_0401;
const JOYSTICK_SOURCE_BITS: u32 = 0x0100_0010;

/// The axes read off each joystick motion event, in [`PadAxes`] order.
const AXES: [Axis; 10] = [
    Axis::X,
    Axis::Y,
    Axis::Z,
    Axis::Rz,
    Axis::Ltrigger,
    Axis::Rtrigger,
    Axis::Brake,
    Axis::Gas,
    Axis::HatX,
    Axis::HatY,
];

type PadAxes = [f32; AXES.len()];

enum PadInput {
    Axes {
        device: i32,
        axes: PadAxes,
    },
    Key {
        device: i32,
        button: GamepadButton,
        down: bool,
    },
}

/// Filled by the input filter inside winit's event pump and drained by
/// [`feed`] on the same thread, before Bevy's input systems run.
static QUEUE: Mutex<Vec<PadInput>> = Mutex::new(Vec::new());

fn push(input: PadInput) {
    QUEUE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(input);
}

fn is_source(source: u32, bits: u32) -> bool {
    source & bits == bits
}

/// Android key codes (`AKEYCODE_*`) a controller sends.
fn button(code: u32) -> Option<GamepadButton> {
    Some(match code {
        19 => GamepadButton::DPadUp,
        20 => GamepadButton::DPadDown,
        21 => GamepadButton::DPadLeft,
        22 => GamepadButton::DPadRight,
        23 | 96 => GamepadButton::South,
        97 => GamepadButton::East,
        99 => GamepadButton::West,
        100 => GamepadButton::North,
        102 => GamepadButton::LeftTrigger,
        103 => GamepadButton::RightTrigger,
        104 => GamepadButton::LeftTrigger2,
        105 => GamepadButton::RightTrigger2,
        106 => GamepadButton::LeftThumb,
        107 => GamepadButton::RightThumb,
        108 => GamepadButton::Start,
        109 => GamepadButton::Select,
        110 => GamepadButton::Mode,
        _ => return None,
    })
}

/// The input filter: takes controller events, leaves the rest (touch, keyboard,
/// and keys like Back that a controller may also send) to winit.
fn claim(event: &InputEvent<'_>) -> bool {
    match event {
        InputEvent::MotionEvent(motion)
            if is_source(motion.source().into(), JOYSTICK_SOURCE_BITS) =>
        {
            let pointer = motion.pointer_at_index(0);
            push(PadInput::Axes {
                device: motion.device_id(),
                axes: AXES.map(|axis| pointer.axis_value(axis)),
            });
            true
        }
        InputEvent::KeyEvent(key) if is_source(key.source().into(), GAMEPAD_SOURCE_BITS) => {
            let Some(button) = button(key.key_code().into()) else {
                return false;
            };
            let down = match key.action() {
                KeyAction::Down if key.repeat_count() == 0 => true,
                KeyAction::Up => false,
                _ => return true,
            };
            push(PadInput::Key {
                device: key.device_id(),
                button,
                down,
            });
            true
        }
        _ => false,
    }
}

struct Pad {
    entity: Entity,
    axes: Vec<(GamepadAxis, f32)>,
    buttons: HashMap<GamepadButton, f32>,
}

#[derive(Default)]
struct Pads(HashMap<i32, Pad>);

impl Pad {
    fn button(&mut self, out: &mut Vec<RawGamepadEvent>, button: GamepadButton, value: f32) {
        if self.buttons.insert(button, value) == Some(value) {
            return;
        }
        out.push(RawGamepadButtonChangedEvent::new(self.entity, button, value).into());
    }

    fn axis(&mut self, out: &mut Vec<RawGamepadEvent>, axis: GamepadAxis, value: f32) {
        match self.axes.iter_mut().find(|(known, _)| *known == axis) {
            Some((_, last)) if *last == value => return,
            Some((_, last)) => *last = value,
            None => self.axes.push((axis, value)),
        }
        out.push(RawGamepadAxisChangedEvent::new(self.entity, axis, value).into());
    }

    /// Android's stick Y points down; Bevy's points up.
    fn motion(&mut self, out: &mut Vec<RawGamepadEvent>, axes: PadAxes) {
        let [x, y, z, rz, ltrigger, rtrigger, brake, gas, hat_x, hat_y] = axes;
        self.axis(out, GamepadAxis::LeftStickX, x);
        self.axis(out, GamepadAxis::LeftStickY, -y);
        self.axis(out, GamepadAxis::RightStickX, z);
        self.axis(out, GamepadAxis::RightStickY, -rz);
        self.button(out, GamepadButton::LeftTrigger2, ltrigger.max(brake));
        self.button(out, GamepadButton::RightTrigger2, rtrigger.max(gas));
        let pressed = |on: bool| if on { 1.0 } else { 0.0 };
        self.button(out, GamepadButton::DPadLeft, pressed(hat_x < -0.5));
        self.button(out, GamepadButton::DPadRight, pressed(hat_x > 0.5));
        self.button(out, GamepadButton::DPadUp, pressed(hat_y < -0.5));
        self.button(out, GamepadButton::DPadDown, pressed(hat_y > 0.5));
    }
}

fn feed(
    mut commands: Commands,
    mut pads: Local<Pads>,
    mut events: MessageWriter<RawGamepadEvent>,
    mut connections: MessageWriter<GamepadConnectionEvent>,
    mut buttons: MessageWriter<RawGamepadButtonChangedEvent>,
    mut axes: MessageWriter<RawGamepadAxisChangedEvent>,
) {
    let inputs = std::mem::take(
        &mut *QUEUE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let mut out = Vec::new();
    for input in inputs {
        let device = match input {
            PadInput::Axes { device, .. } | PadInput::Key { device, .. } => device,
        };
        let pad = pads.0.entry(device).or_insert_with(|| {
            let entity = commands.spawn_empty().id();
            let event = GamepadConnectionEvent::new(
                entity,
                GamepadConnection::Connected {
                    name: format!("Android gamepad {device}"),
                    vendor_id: None,
                    product_id: None,
                },
            );
            diag::info!(Launch, "gamepad: Android device {device} connected");
            out.push(event.into());
            Pad {
                entity,
                axes: Vec::new(),
                buttons: HashMap::new(),
            }
        });
        match input {
            PadInput::Axes { axes, .. } => pad.motion(&mut out, axes),
            PadInput::Key { button, down, .. } => {
                pad.button(&mut out, button, if down { 1.0 } else { 0.0 });
            }
        }
    }
    // `bevy_gilrs` writes each raw event twice — once into the ordered stream,
    // once into its own kind's channel — and Bevy reads both.
    for event in out {
        match &event {
            RawGamepadEvent::Connection(connection) => {
                connections.write(connection.clone());
            }
            RawGamepadEvent::Button(button) => {
                buttons.write(*button);
            }
            RawGamepadEvent::Axis(axis) => {
                axes.write(*axis);
            }
        }
        events.write(event);
    }
}

pub(crate) struct AndroidGamepadPlugin;

impl Plugin for AndroidGamepadPlugin {
    fn build(&self, app: &mut App) {
        if !android_activity::input_filter::set(claim) {
            diag::warn!(Launch, "gamepad: an Android input filter was already set");
        }
        app.add_systems(PreUpdate, feed.before(InputSystems));
    }
}
