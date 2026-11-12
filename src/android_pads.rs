//! Android: which controller is which player, and what each one means by its face buttons.
//!
//! Android hands the player one stream of key and motion events with a device id. A handheld's own pad is one
//! device, a Bluetooth or USB pad another. `Pads` knows the gamepads that are connected (the Java side sends the
//! roster whenever `InputManager` reports a change), seats them with the same rules as the handhelds' couch co-op
//! (`players::Seats`: the Pocket first, then in the order they connected; a pad that goes away leaves its seat
//! empty and gets it back; a new one takes the lowest seat nobody had in this game), and keeps the buttons of
//! players 2 to 4. Player 1's buttons stay with the player (touch, pulses, hold-to-toggle live there).
//!
//! - A core that lists only one player port (Game Boy, GBA, most consoles' single-player cores) gets every pad as
//!   player 1: a Bluetooth pad picked up next to the handheld just works, nobody has to find a seat.
//! - Menus take every pad, seated or not.
//! - `pad::android::Arrangement` says where a pad's A/B/X/Y sit (Odin: by its Controller style setting).
use crate::pad::android::{arrangement, is_odin_pad, retro_bit_on, Arrangement, Trigger};
use crate::pad::{Controller, BUILTIN};
use crate::players::{connected_note, disconnected_note, Mode, Seats};

/// One gamepad as `InputDevice` describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: i32,
    /// `InputDevice.getDescriptor()`: the same across reconnects.
    pub descriptor: String,
    pub name: String,
    pub vendor: u16,
    pub product: u16,
}

impl Device {
    /// `id\tdescriptor\tname\tvendor\tproduct`, one device a line (what `PlayerActivity.roster` sends).
    pub fn parse(line: &str) -> Option<Device> {
        let mut f = line.split('\t');
        let id = f.next()?.trim().parse().ok()?;
        let descriptor = f.next()?.to_string();
        let name = f.next()?.to_string();
        let vendor = f.next()?.trim().parse().ok()?;
        let product = f.next()?.trim().parse().ok()?;
        Some(Device { id, descriptor, name, vendor, product })
    }

    fn controller(&self, taken: &[Controller]) -> Controller {
        // The Odin's own pad is one controller in either Controller style (the style switch shows another device).
        // Others: make, model and name, not `descriptor`: Android adds a counter to a descriptor that is in use, so the
        // same pad can come back with another one.
        let mut key = if is_odin_pad(self.vendor, self.product) { BUILTIN.to_string() } else { format!("android:{:04x}:{:04x}:{}", self.vendor, self.product, self.name) };
        // Two pads that look alike (no serial number) must not become one player.
        while taken.iter().any(|c| c.key == key) {
            key.push('+');
        }
        Controller { key, name: self.name.clone(), gamepad: true, node: String::new() }
    }
}

struct Slot {
    device: Device,
    controller: Controller,
    arrangement: Arrangement,
    /// Held keys, and the buttons the hat, the sticks and the analog triggers hold (apart: a button held by both lets
    /// go only when both do).
    keys: u16,
    analog: u16,
    triggers: [Trigger; 2],
    sticks: [i16; 4],
}

/// The connected gamepads, their seats and the buttons of the players that are not player 1.
pub struct Pads {
    slots: Vec<Slot>,
    seats: Seats,
    /// The core lists more than one player port: only then are pads seated apart.
    multi: bool,
    /// The first roster is in (later changes are announced).
    started: bool,
}

impl Default for Pads {
    fn default() -> Self {
        Pads { slots: Vec::new(), seats: Seats::default(), multi: false, started: false }
    }
}

impl Pads {
    /// Whether the core has ports for more than one player.
    pub fn set_multiplayer(&mut self, multi: bool) {
        self.multi = multi;
    }

    /// The roster as of now (all connected gamepads, oldest first).
    /// Returns the notes for the player: a pad that came or went, and the player it is.
    pub fn roster(&mut self, mut devices: Vec<Device>) -> Vec<String> {
        devices.sort_by_key(|d| d.id);
        devices.dedup_by_key(|d| d.id);
        let mut notes = Vec::new();
        let old: Vec<Slot> = std::mem::take(&mut self.slots);
        let mut controllers: Vec<Controller> = Vec::new();
        for d in &devices {
            let c = d.controller(&controllers);
            controllers.push(c);
        }
        // Keep what a pad that stays is holding; a new pad starts at rest.
        let mut gone: Vec<Controller> = old.iter().map(|s| s.controller.clone()).filter(|c| !controllers.iter().any(|n| n.key == c.key)).collect();
        for (d, c) in devices.iter().zip(&controllers) {
            let mut slot = Slot { arrangement: arrangement(d.vendor, d.product), device: d.clone(), controller: c.clone(), keys: 0, analog: 0, triggers: [Trigger::default(); 2], sticks: [0; 4] };
            if let Some(before) = old.iter().find(|s| s.controller.key == c.key) {
                slot.keys = before.keys;
                slot.analog = before.analog;
                slot.triggers = before.triggers;
                slot.sticks = before.sticks;
            }
            self.slots.push(slot);
        }
        if !self.started {
            // An empty first roster (the player started before Android listed anything) is no first look yet.
            if controllers.is_empty() {
                return notes;
            }
            self.seats = Seats::arrange(&[], &controllers, Mode::Compact);
            self.started = true;
            return notes;
        }
        for c in gone.drain(..) {
            if let Some(p) = self.seats.disconnect(&c.key) {
                notes.push(disconnected_note(&c, p));
            }
        }
        for c in &controllers {
            if old.iter().any(|s| s.controller.key == c.key) {
                // Known already; a different device id of the same pad (it was switched off and on) keeps its seat.
                self.seats.connect(c);
                continue;
            }
            let player = self.seats.connect(c);
            notes.push(connected_note(c, player));
        }
        notes
    }

    fn slot(&self, id: i32) -> Option<&Slot> {
        self.slots.iter().find(|s| s.device.id == id)
    }

    /// 0-based player of this device. A device nobody introduced (the system's Back key, a keyboard, `adb input`)
    /// is player 1; a core with one port gives everything to player 1; a pad without a seat plays nobody (`None`).
    pub fn player_of(&self, id: i32) -> Option<usize> {
        if !self.multi {
            return Some(0);
        }
        match self.slot(id) {
            Some(s) => self.seats.player_of(&s.controller.key),
            None => Some(0),
        }
    }

    /// Where this device has its A/B/X/Y.
    pub fn arrangement_of(&self, id: i32) -> Arrangement {
        self.slot(id).map_or(Arrangement::Xbox, |s| s.arrangement)
    }

    /// A key of a pad that plays player 2 to 4.
    pub fn key(&mut self, id: i32, key: i32, down: bool) {
        let arrangement = self.arrangement_of(id);
        let Some(s) = self.slots.iter_mut().find(|s| s.device.id == id) else { return };
        if let Some(bit) = retro_bit_on(key, arrangement) {
            if down { s.keys |= 1 << bit } else { s.keys &= !(1 << bit) }
        }
    }

    /// The sticks, hat and analog triggers of a pad that plays player 2 to 4. `stick_dpad`: the left stick is also the
    /// D-pad (systems without an analog stick).
    pub fn motion(&mut self, id: i32, axes: [f32; 4], hat: [f32; 2], triggers: [f32; 2], stick_dpad: bool) {
        let Some(s) = self.slots.iter_mut().find(|s| s.device.id == id) else { return };
        for (latch, value) in s.triggers.iter_mut().zip(triggers) {
            latch.update(value);
        }
        s.sticks = axes.map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16);
        let x = if hat[0].abs() > 0.5 { hat[0] } else { axes[0] };
        let y = if hat[1].abs() > 0.5 { hat[1] } else { axes[1] };
        let (x, y) = if stick_dpad { (x, y) } else { (hat[0], hat[1]) };
        // The D-pad bits, the triggers' bits: the same thresholds as player 1 (`PlayerActivity`'s `analog`).
        s.analog = (if x < -0.55 { 1 << 6 } else if x > 0.55 { 1 << 7 } else { 0 })
            | (if y < -0.55 { 1 << 4 } else if y > 0.55 { 1 << 5 } else { 0 })
            | (if s.triggers[0].down() { 1 << 12 } else { 0 })
            | (if s.triggers[1].down() { 1 << 13 } else { 0 });
    }

    /// Player `p`'s (1..3) buttons and sticks for the core.
    pub fn player(&self, p: usize) -> (u16, [i16; 4]) {
        let Some(c) = self.seats.get(p) else { return (0, [0; 4]) };
        self.slots.iter().find(|s| s.controller.key == c.key).map_or((0, [0; 4]), |s| (s.keys | s.analog, s.sticks))
    }

    /// Every button let go (a pause, a pad that went away): nothing stays held.
    pub fn release_all(&mut self) {
        for s in &mut self.slots {
            s.keys = 0;
            s.analog = 0;
            s.triggers = [Trigger::default(); 2];
            s.sticks = [0; 4];
        }
    }

    pub fn seats(&self) -> &Seats {
        &self.seats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::android::{button, retro_bit, retro_bit_on};
    use crate::pad::Button;

    fn dev(id: i32, descriptor: &str, name: &str, vendor: u16, product: u16) -> Device {
        Device { id, descriptor: descriptor.into(), name: name.into(), vendor, product }
    }
    /// The Odin's own pad as its input service shows it (`Xbox Wireless Controller`, 2020:0112).
    fn odin(id: i32) -> Device {
        dev(id, "odin", "Xbox Wireless Controller", 0x2020, 0x0112)
    }
    fn blue(id: i32) -> Device {
        dev(id, "bt-a", "8BitDo Pro 2", 0x2dc8, 0x6003)
    }
    fn blue_b(id: i32) -> Device {
        dev(id, "bt-b", "DualSense Wireless Controller", 0x054c, 0x0ce6)
    }
    const SOUTH_BUTTON: i32 = 96;
    const EAST_BUTTON: i32 = 97;
    const R2_KEY: i32 = 105;

    #[test]
    fn confirm_stays_confirm_whatever_the_pad_swaps() {
        // Menus take Android's A as the confirming button and B as back, on every pad: which physical button that is
        // is the device's own setting. Nothing about the face arrangement reaches the menus.
        assert_eq!(button(96), Some(Button::East));
        assert_eq!(button(97), Some(Button::South));
        assert_eq!(button(23), Some(Button::East), "the centre key confirms too");
    }

    #[test]
    fn in_the_game_a_button_keeps_its_place_like_the_original() {
        // libretro: B bottom (0), A right (8), Y left (1), X top (9). Xbox letters: A bottom, B right, X left, Y top.
        let xbox = |k| retro_bit_on(k, Arrangement::Xbox);
        assert_eq!((xbox(96), xbox(97), xbox(99), xbox(100)), (Some(0), Some(8), Some(1), Some(9)));
        // Nintendo letters: A right, B bottom, X top, Y left.
        let nin = |k| retro_bit_on(k, Arrangement::Nintendo);
        assert_eq!((nin(96), nin(97), nin(99), nin(100)), (Some(8), Some(0), Some(9), Some(1)));
        assert_eq!(retro_bit(96), Some(0), "an unknown pad is read as Android's own convention");
        // The same place gives the same console button, whichever letters the pad prints there.
        assert_eq!(xbox(96), nin(97), "bottom");
        assert_eq!(xbox(97), nin(96), "right");
        assert_eq!(xbox(99), nin(100), "left");
        assert_eq!(xbox(100), nin(99), "top");
    }

    #[test]
    fn the_odin_is_read_by_its_controller_style_device_and_other_pads_by_make() {
        // The Odin's style switch shows another device: 0112 "Xbox Wireless Controller" is Xbox style (flip_button_layout 1),
        // 0111 "Odin Controller" the Odin style (0), with its own Nintendo lettering.
        assert_eq!(arrangement(0x2020, 0x0112), Arrangement::Xbox);
        assert_eq!(arrangement(0x2020, 0x0111), Arrangement::Nintendo);
        // A Switch Pro Controller (057e:2009) has A on the right.
        assert_eq!(arrangement(0x057e, 0x2009), Arrangement::Nintendo);
        assert_eq!(arrangement(0x045e, 0x028e), Arrangement::Xbox);
        assert_eq!(arrangement(0x054c, 0x0ce6), Arrangement::Xbox);
    }

    #[test]
    fn switching_the_odins_controller_style_mid_game_keeps_player_one() {
        // Seen on the Odin 2 Mini (11.11.2026): the style switch removes "Xbox Wireless Controller" and adds "Odin
        // Controller"; the new device took player 2 and the game stopped hearing the pad.
        let odin_style = dev(10, "odin2", "Odin Controller", 0x2020, 0x0111);
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![odin(8)]);
        assert_eq!(pads.player_of(8), Some(0));
        pads.roster(vec![odin_style.clone()]);
        assert_eq!(pads.player_of(10), Some(0), "still player 1");
        assert_eq!(pads.arrangement_of(10), Arrangement::Nintendo);
        pads.roster(vec![odin(11)]);
        assert_eq!(pads.player_of(11), Some(0));
        assert_eq!(pads.arrangement_of(11), Arrangement::Xbox);
        // And while a second pad plays along it keeps player 2.
        pads.roster(vec![odin(11), blue(12)]);
        pads.roster(vec![blue(12), odin_style]);
        assert_eq!((pads.player_of(10), pads.player_of(12)), (Some(0), Some(1)));
    }

    #[test]
    fn a_pad_keeps_its_seat_when_android_gives_it_another_descriptor() {
        // Android adds a counter to a descriptor that is in use: the same Switch Pro came back as another "device".
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![odin(3), dev(9, "abc", "Pro Controller", 0x057e, 0x2009)]);
        pads.roster(vec![odin(3), dev(9, "abc-1", "Pro Controller", 0x057e, 0x2009)]);
        assert_eq!(pads.player_of(9), Some(1));
    }

    #[test]
    fn a_second_pad_is_player_two_and_the_handhelds_own_stays_player_one() {
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        // A Bluetooth pad that Android listed before the Odin's own still does not take player 1.
        pads.roster(vec![blue(9), odin(3)]);
        assert_eq!(pads.player_of(3), Some(0), "the Pocket first");
        assert_eq!(pads.player_of(9), Some(1));
        pads.key(9, SOUTH_BUTTON, true);
        assert_eq!(pads.player(1).0, 1 << 0, "player 2's south button (libretro B)");
        pads.key(9, SOUTH_BUTTON, false);
        assert_eq!(pads.player(1).0, 0);
    }

    #[test]
    fn an_empty_first_roster_is_not_the_first_look() {
        // The game can start before Android has listed any pad: the pads that come after are seated together, the
        // handheld's own first, and are not announced one by one.
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        assert!(pads.roster(vec![]).is_empty());
        let notes = pads.roster(vec![blue(2), odin(3)]);
        assert!(notes.is_empty());
        assert_eq!((pads.player_of(3), pads.player_of(2)), (Some(0), Some(1)));
    }

    #[test]
    fn a_pad_alone_is_player_one() {
        // A phone with a Bluetooth pad, or a handheld whose own pad is off ("Disconnect" controller style).
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![blue(7)]);
        assert_eq!(pads.player_of(7), Some(0));
    }

    #[test]
    fn a_game_with_one_port_hears_every_pad_as_player_one() {
        let mut pads = Pads::default();
        pads.set_multiplayer(false);
        pads.roster(vec![odin(3), blue(9)]);
        assert_eq!((pads.player_of(3), pads.player_of(9)), (Some(0), Some(0)));
    }

    #[test]
    fn a_device_nobody_introduced_is_player_one() {
        // The system's Back key, `adb shell input`, a keyboard.
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![odin(3), blue(9)]);
        assert_eq!(pads.player_of(-1), Some(0));
        assert_eq!(pads.arrangement_of(-1), Arrangement::Xbox);
    }

    #[test]
    fn a_pad_that_goes_and_comes_back_mid_game_is_the_same_player() {
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        assert!(pads.roster(vec![odin(3), blue(9)]).is_empty(), "the first look is silent");
        pads.key(9, EAST_BUTTON, true);
        assert_eq!(pads.player(1).0, 1 << 8);
        // It is switched off while a button is down: that player has nothing, and a button stays up when it returns.
        let notes = pads.roster(vec![odin(3)]);
        assert_eq!(notes, vec!["8BitDo Pro 2 disconnected — Player 2".to_string()]);
        pads.release_all();
        assert_eq!(pads.player(1), (0, [0; 4]));
        // A second pad appears meanwhile: it takes the seat nobody had in this game, not the absent pad's.
        let notes = pads.roster(vec![odin(3), blue_b(12)]);
        assert_eq!(notes, vec!["DualSense connected — Player 3".to_string()]);
        assert_eq!(pads.player_of(12), Some(2));
        // The first one comes back with another device id: Player 2 again.
        let notes = pads.roster(vec![odin(3), blue_b(12), blue(15)]);
        assert_eq!(notes, vec!["8BitDo Pro 2 connected — Player 2".to_string()]);
        assert_eq!(pads.player_of(15), Some(1));
        assert_eq!(pads.player_of(12), Some(2));
    }

    #[test]
    fn the_pad_a_player_was_holding_is_not_held_by_the_next_one() {
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![odin(3), blue(9)]);
        pads.key(9, SOUTH_BUTTON, true);
        pads.roster(vec![odin(3)]);
        pads.roster(vec![odin(3), blue_b(12)]);
        assert_eq!(pads.player(1), (0, [0; 4]), "Player 2 has no pad");
        assert_eq!(pads.player(2).0, 0);
        pads.roster(vec![odin(3), blue_b(12), blue(14)]);
        assert_eq!(pads.player(1).0, 0, "the returning pad starts at rest");
    }

    #[test]
    fn a_pad_set_to_nintendo_letters_keeps_its_places_as_player_two() {
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        let pro = dev(5, "pro", "Pro Controller", 0x057e, 0x2009);
        pads.roster(vec![odin(3), pro]);
        assert_eq!(pads.arrangement_of(5), Arrangement::Nintendo);
        pads.key(5, SOUTH_BUTTON, true);
        assert_eq!(pads.player(1).0, 1 << 8, "BUTTON_A is the right-hand button on this pad: the console's A");
    }

    #[test]
    fn analog_triggers_of_a_second_pad_press_and_let_go_like_the_first() {
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![odin(3), blue(9)]);
        pads.motion(9, [0.0; 4], [0.0; 2], [0.43, 0.9], false);
        assert_eq!(pads.player(1).0, 1 << 13, "R2 pressed, L2 resting part way in is not");
        pads.motion(9, [0.0; 4], [0.0; 2], [0.0, 0.6], false);
        assert_eq!(pads.player(1).0, 1 << 13, "easing off keeps it held");
        pads.motion(9, [0.0; 4], [0.0; 2], [0.0, 0.2], false);
        assert_eq!(pads.player(1).0, 0);
    }

    #[test]
    fn a_digital_and_an_analog_trigger_together_let_go_when_both_do() {
        // The Odin's "L2/R2 mode: both" sends the key and the axis.
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![odin(3), blue(9)]);
        pads.key(9, R2_KEY, true);
        pads.motion(9, [0.0; 4], [0.0; 2], [0.0, 1.0], false);
        pads.key(9, R2_KEY, false);
        assert_eq!(pads.player(1).0, 1 << 13, "the axis still holds it");
        pads.motion(9, [0.0; 4], [0.0; 2], [0.0, 0.0], false);
        assert_eq!(pads.player(1).0, 0);
    }

    #[test]
    fn the_hat_is_the_d_pad_and_the_stick_goes_to_the_core() {
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![odin(3), blue(9)]);
        pads.motion(9, [0.5, -1.0, 0.0, 0.0], [1.0, 0.0], [0.0; 2], false);
        let (bits, sticks) = pads.player(1);
        assert_eq!(bits, 1 << 7, "a console with an analog stick keeps the stick analog: only the hat presses");
        assert_eq!(sticks[1], -32767);
        pads.motion(9, [0.0, -1.0, 0.0, 0.0], [0.0, 0.0], [0.0; 2], true);
        assert_eq!(pads.player(1).0, 1 << 4, "on a console without one the stick is the D-pad");
    }

    #[test]
    fn two_pads_that_look_alike_are_two_players() {
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        pads.roster(vec![blue(4), blue(6)]);
        assert_eq!((pads.player_of(4), pads.player_of(6)), (Some(0), Some(1)));
    }

    #[test]
    fn a_fifth_pad_waits_and_plays_nobody() {
        let mut pads = Pads::default();
        pads.set_multiplayer(true);
        let list: Vec<Device> = (0..5).map(|i| dev(10 + i, &format!("d{i}"), "Pad", 1, 1)).collect();
        pads.roster(list);
        assert_eq!(pads.player_of(14), None);
        pads.key(14, SOUTH_BUTTON, true);
        assert_eq!((1..4).map(|p| pads.player(p).0).sum::<u16>(), 0);
    }

    #[test]
    fn the_roster_line_from_java_is_read() {
        let d = Device::parse("8\tabc123\tXbox Wireless Controller\t8224\t274").unwrap();
        assert_eq!((d.id, d.vendor, d.product, d.name.as_str()), (8, 0x2020, 0x0112, "Xbox Wireless Controller"));
        assert!(Device::parse("8\tabc").is_none());
    }
}
