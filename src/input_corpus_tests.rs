//! Captured descriptors through the same decoder, controller merge, menu roles and
//! hosted-game input used on a handheld. No uinput device or physical pad needed.
#![cfg(target_os = "linux")]
use evdev::InputEvent;
use crate::button_layout::{ButtonLayout, Remap};
use crate::hotkeys::{Action, Enabled, Hotkeys};
use crate::pad::{decode::Decoder, Button, Controller, PadEvent, PadState};
use crate::players::Inputs;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture { provenance: String, nodes: Vec<Node> }
#[derive(Deserialize)]
struct Id { bustype: u16, vendor: u16, product: u16, version: u16 }
#[derive(Deserialize)]
struct Node { name: String, id: Id, keys: Vec<u16>, abs: Vec<[i32; 6]>, bitmaps: std::collections::BTreeMap<String, String>, expected_keys: Vec<Key>, #[serde(default)] expected_axes: Vec<AxisCase> }
#[derive(Deserialize)]
struct Key { code: u16, physical: Option<Button>, menu_nintendo: Option<Button>, menu_xbox: Option<Button>, retro_mask: u16 }
#[derive(Deserialize)]
struct AxisCase { code: u16, axis: Option<AxisName>, hat: Option<String> }
#[derive(Clone,Copy,Deserialize)]
#[serde(rename_all="SCREAMING_SNAKE_CASE")]
enum AxisName { LeftX, LeftY, RightX, RightY, L2, R2 }
impl AxisName {
    fn value(self, p: &PadState) -> f32 { match self { Self::LeftX=>p.left.0, Self::LeftY=>p.left.1, Self::RightX=>p.right.0, Self::RightY=>p.right.1, Self::L2=>p.l2, Self::R2=>p.r2 } }
    fn trigger(self) -> Option<Button> { match self {Self::L2=>Some(Button::L2),Self::R2=>Some(Button::R2),_=>None} }
}
struct Path { decoder: Decoder, menu: PadState, roles: Remap, input: Inputs, hotkeys: Hotkeys, layout: ButtonLayout }
impl Path {
    fn new(node: &Node, layout: ButtonLayout) -> Self {
        let decoder=Decoder::with_caps(node.id.bustype,node.id.vendor,node.id.product,node.id.version,&node.name,node.keys.clone(),node.abs.iter().map(|a|(a[0] as u16,a[1],a[2])).collect());
        let mut menu=PadState::new(vec![]);
        let input=Inputs::new(vec![],None);
        let connected=PadEvent::Connected { source:0,controller:Controller::builtin() };
        menu.merge(connected.clone()); input.feed(connected, |_|{}); input.arrange();
        let mut hotkeys=Hotkeys::new(Enabled::default());
        #[cfg(feature="brick")]
        {
            // The same settings load as pocket-core, rather than hard-coded actions.
            let dir=tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("brick-keys.json"),r#"{"f1":"save","f2":"load"}"#).unwrap();
            let keys=crate::brick_hw::FKeys::load(dir.path());
            hotkeys.set_f_keys([crate::hotkeys::action_of(keys.f1),crate::hotkeys::action_of(keys.f2)]);
        }
        // Keep this mutable in both feature builds; it is fed below.
        let _ = &mut hotkeys;
        Self { decoder,menu,roles:Remap::default(),input,hotkeys,layout }
    }
    fn feed(&mut self, kind:u16, code:u16, value:i32) -> Vec<PadEvent> {
        let events=self.decoder.event(0,InputEvent::new(kind,code,value));
        for e in &events {
            self.input.feed(e.clone(),|merged| self.hotkeys.feed(merged));
            let role=self.roles.event_with(self.layout,e.clone());
            self.menu.merge(role);
        }
        events
    }
}
fn bits(node: &Node, field: &str) -> Vec<u16> {
    if let Some(bytes) = node.bitmaps.get(&format!("{field}_bytes_le")) {
        (0..bytes.len()/2).flat_map(|i| {
            let byte=u8::from_str_radix(&bytes[i*2..i*2+2],16).unwrap();
            (0..8).filter(move |b| byte & (1<<b) != 0).map(move |b| (i*8+b) as u16)
        }).collect()
    } else if let Some(hex) = node.bitmaps.get(&format!("{field}_hex")) {
        hex.chars().rev().enumerate().flat_map(|(i,c)| {
            let nibble=c.to_digit(16).unwrap();
            (0..4).filter(move |b| nibble & (1<<b) != 0).map(move |b| (i*4+b) as u16)
        }).collect()
    } else {
        node.bitmaps[field].split_whitespace().rev().enumerate().flat_map(|(i,word)| {
            let word=u64::from_str_radix(word,16).unwrap();
            (0..64).filter(move |b| word & (1<<b) != 0).map(move |b| (i*64+b) as u16)
        }).collect()
    }
}
fn check(file:&str) {
    let bytes=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/input").join(file)).unwrap();
    let fixture:Fixture=serde_json::from_slice(&bytes).unwrap();
    assert!(!fixture.provenance.is_empty());
    for node in fixture.nodes {
        assert_eq!(bits(&node,"key"),node.keys,"{} key list differs from captured bitmap",node.name);
        assert_eq!(bits(&node,"abs"),node.abs.iter().map(|a|a[0] as u16).collect::<Vec<_>>(),"{} axis list differs from captured bitmap",node.name);
        // Every advertised key and axis must be accounted for, including explicitly unused ones.
        assert_eq!(node.keys,node.expected_keys.iter().map(|k|k.code).collect::<Vec<_>>(),"{} keys missing from the evidence",node.name);
        assert_eq!(node.abs.iter().map(|a|a[0] as u16).collect::<Vec<_>>(),node.expected_axes.iter().map(|a|a.code).collect::<Vec<_>>(),"{} axes missing",node.name);
        for layout in [ButtonLayout::Nintendo,ButtonLayout::Xbox] {
            let mut path=Path::new(&node,layout);
            for k in &node.expected_keys {
                for down in [true,false] {
                    path.feed(1,k.code,i32::from(down));
                    let role=if layout==ButtonLayout::Nintendo {k.menu_nintendo} else {k.menu_xbox};
                    for b in Button::ALL { assert_eq!(path.menu.is_pressed(b),down && role==Some(b),"{} key {} menu {layout:?} {b:?}",node.name,k.code); }
                    assert_eq!(path.input.buttons(0),if down {k.retro_mask} else {0},"{} key {} must keep its game button in both menu layouts",node.name,k.code);
                    let actions=path.hotkeys.take();
                    let expected=match (down,k.physical) {
                        #[cfg(feature="brick")]
                        (true,Some(Button::F1))=>vec![Action::Save],
                        #[cfg(feature="brick")]
                        (true,Some(Button::F2))=>vec![Action::Load],
                        _=>vec![],
                    };
                    assert_eq!(actions,expected,"{} key {} shortcut",node.name,k.code);
                }
            }
            for case in &node.expected_axes {
                let a=node.abs.iter().find(|a|a[0]==i32::from(case.code)).unwrap();
                if let Some(axis)=case.axis {
                    for (value,want) in [(a[1],if axis.trigger().is_some(){0.0}else{-1.0}),(a[2],1.0),(a[1],if axis.trigger().is_some(){0.0}else{-1.0})] {
                        path.feed(3,case.code,value);
                        assert!((axis.value(&path.menu)-want).abs()<0.01,"{} axis {} menu: {} != {want}",node.name,case.code,axis.value(&path.menu));
                        if let Some(b)=axis.trigger() {
                            assert_eq!(path.menu.is_pressed(b),want>0.5,"{} axis {} trigger",node.name,case.code);
                            let mask=if b==Button::L2 {1<<12}else{1<<13};
                            assert_eq!(path.input.buttons(0)&mask!=0,want>0.5,"{} axis {} game trigger",node.name,case.code);
                        } else {
                            let index=match axis {AxisName::LeftX=>0,AxisName::LeftY=>1,AxisName::RightX=>2,AxisName::RightY=>3,_=>unreachable!()};
                            let actual=path.input.sticks(0).analog(true)[index];
                            assert_eq!(actual.signum(),if want>0.0 {1}else{-1},"{} axis {} game stick",node.name,case.code);
                        }
                    }
                    path.feed(3,case.code,if axis.trigger().is_some(){a[1]}else{(a[1]+a[2])/2});
                } else if let Some(hat)=&case.hat {
                    let buttons=if hat=="HORIZONTAL" {[Button::Left,Button::Right]}else{[Button::Up,Button::Down]};
                    for (value,b) in [(-1,buttons[0]),(1,buttons[1])] {
                        path.feed(3,case.code,value);
                        assert!(path.menu.is_pressed(b),"{} D-pad {b:?}",node.name);
                        assert_ne!(path.input.buttons(0),0,"{} game D-pad {b:?}",node.name);
                        path.feed(3,case.code,0);
                        assert!(!path.menu.is_pressed(b)); assert_eq!(path.input.buttons(0),0);
                    }
                } else {
                    assert!(path.feed(3,case.code,a[2]).is_empty(),"{} unused axis {} must not press a trigger",node.name,case.code);
                }
            }
            // A trigger reported as both a key and an axis must survive either
            // release order and produce only one held game/menu button.
            for case in &node.expected_axes {
                let Some(button) = case.axis.and_then(AxisName::trigger) else { continue };
                let Some(key) = node.expected_keys.iter().find(|k| k.physical == Some(button)) else { continue };
                let range = node.abs.iter().find(|a| a[0] == i32::from(case.code)).unwrap();
                let mask = if button == Button::L2 { 1 << 12 } else { 1 << 13 };
                for key_first in [true, false] {
                    path.feed(1, key.code, 1);
                    path.feed(3, case.code, range[2]);
                    assert!(path.menu.is_pressed(button));
                    assert_ne!(path.input.buttons(0) & mask, 0);
                    if key_first { path.feed(1, key.code, 0); }
                    else { path.feed(3, case.code, range[1]); }
                    assert!(path.menu.is_pressed(button), "{} trigger survives first release", node.name);
                    assert_ne!(path.input.buttons(0) & mask, 0);
                    if key_first { path.feed(3, case.code, range[1]); }
                    else { path.feed(1, key.code, 0); }
                    assert!(!path.menu.is_pressed(button));
                    assert_eq!(path.input.buttons(0) & mask, 0);
                }
            }
            // MENU + shoulder shortcuts must still work after every physical input has been released.
            // Keyboard-only companion nodes have no shoulders and are covered by their menu-role assertions.
            if let (Some(home),Some(save))=(node.expected_keys.iter().find(|k|k.physical==Some(Button::Mode)),node.expected_keys.iter().find(|k|k.physical==Some(Button::R1))) {
                path.feed(1,home.code,1);path.feed(1,save.code,1);path.feed(1,save.code,0);path.feed(1,home.code,0);
                assert_eq!(path.hotkeys.take(),vec![Action::Save],"{} MENU + R1",node.name);
            }
        }
    }
}
#[test] fn rg35xx_plus(){check("rg35xx-plus.json")}
#[test] fn retroid_mini_native(){check("retroid-mini-native.json")}
#[test] fn odin_native(){check("odin-native.json")}
#[test] fn retroid_mini_inputplumber(){check("retroid-mini-inputplumber.json")}
#[test] fn odin_inputplumber(){check("odin-inputplumber.json")}
#[test] fn lite2_d(){check("8bitdo-lite2-d.json")}
#[test] fn lite2_s(){check("8bitdo-lite2-s.json")}
#[test]
#[cfg(feature="brick")]
fn trimui_brick(){check("trimui-brick.json")}
