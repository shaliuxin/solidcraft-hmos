//! 平台输入 → egui 事件。首版移植聚焦指针/触摸/滚轮/键盘；手写笔（pen）留作后续工作。
use egui::{Event, Key, Modifiers, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase};
use serde::Deserialize;
use std::collections::BTreeMap;

/// OHOS 的轴事件一档滚轮约报 6 个单位，egui 的标准一档约 50 点，故乘 8 换算。
const WHEEL_POINTS_PER_UNIT: f32 = 8.0;

/// 平台坐标（物理像素）在 ArkTS 侧上报；egui 事件用点（除以 scale）。
#[derive(Deserialize, Default)]
pub struct Contact {
    pub id: u64,
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize, Default)]
pub struct Input {
    pub kind: String,
    #[serde(default)]
    pub points: Vec<Contact>,
    #[serde(default)]
    pub zoom: f32,
    #[serde(default)]
    pub dx: f32,
    #[serde(default)]
    pub dy: f32,
    #[serde(default)]
    pub action: i32, // 0 down, 1 up, 2 move, 3 cancel
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    /// 触摸抬起时为 1，表示 `x`/`y` 是抬起点的位置（桥在 UP 事件里单独带出，
    /// 因为 UP 的触点不在 `points` 里）。
    #[serde(default)]
    pub up: i32,
    #[serde(default)]
    pub button: i32,
    #[serde(default)]
    pub code: i32,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
}

#[derive(Default)]
pub struct State {
    pressed: Option<PointerButton>,
    last_pointer: Option<Pos2>,
    touches: BTreeMap<u64, Pos2>,
    touch_pointer: Option<u64>,
    pub modifiers: Modifiers,
}

impl State {
    fn move_pointer(&mut self, pos: Pos2, events: &mut Vec<Event>) {
        self.last_pointer = Some(pos);
        events.push(Event::PointerMoved(pos));
    }

    pub fn cancel(&mut self, events: &mut Vec<Event>) {
        let release = self.pressed.take().zip(self.last_pointer.take());
        self.touches.clear();
        self.touch_pointer = None;
        self.modifiers = Modifiers::NONE;
        if let Some((button, pos)) = release {
            events.push(Event::PointerButton {
                pos,
                button,
                pressed: false,
                modifiers: self.modifiers,
            });
        }
        events.push(Event::PointerGone);
    }

    pub fn push(&mut self, input: Input, scale: f32, events: &mut Vec<Event>) {
        if input.kind == "blur" {
            self.cancel(events);
            return;
        }
        if (!input.x.is_finite() || !input.y.is_finite()) || !scale.is_finite() || scale <= 0.0 {
            return;
        }
        if input.kind == "key" {
            let old = self.modifiers;
            self.modifiers = Modifiers {
                ctrl: input.ctrl,
                shift: input.shift,
                alt: input.alt,
                command: input.ctrl,
                ..Default::default()
            };
            if old != self.modifiers {
                events.push(Event::ModifiersChanged(self.modifiers));
            }
        }
        let pos = Pos2::new(input.x / scale, input.y / scale);
        match input.kind.as_str() {
            "touch" => {
                let next: BTreeMap<_, _> = input
                    .points
                    .iter()
                    .map(|p| (p.id, Pos2::new(p.x / scale, p.y / scale)))
                    .collect();
                // 抬起点：桥在 UP 事件里把位置放在顶层 x/y（UP 的触点不进 points）。
                let up_pos = if input.up != 0 {
                    Some(Pos2::new(input.x / scale, input.y / scale))
                } else {
                    None
                };
                for (&id, &p) in &self.touches {
                    if !next.contains_key(&id) {
                        let pos = up_pos.unwrap_or(p);
                        events.push(Event::Touch {
                            device_id: TouchDeviceId(0),
                            id: TouchId(id),
                            phase: TouchPhase::End,
                            pos,
                            force: None,
                        });
                        if self.touch_pointer == Some(id) {
                            if let Some(button) = self.pressed.take() {
                                // 先把指针挪到抬起点，再 release：egui 的 click 判定用的是
                                // 释放瞬间的指针位置，位置缺失会让 `clicked()` 永远为假。
                                // 这里只动字段而不调 self.move_pointer()：外层正借着
                                // &self.touches 迭代，整借 &mut self 会冲突。
                                self.last_pointer = Some(pos);
                                events.push(Event::PointerMoved(pos));
                                events.push(Event::PointerButton {
                                    pos,
                                    button,
                                    pressed: false,
                                    modifiers: self.modifiers,
                                });
                            }
                            // 注意：这里不能紧跟着发 PointerGone。egui 在 PointerGone 里会
                            // 清空 interact_pos，同一帧内释放 + 消失会导致按下有高亮、
                            // 抬起却不触发 click（菜单栏/ribbon 点击失效的根因）。
                            self.last_pointer = Some(pos);
                            self.touch_pointer = None;
                        }
                    }
                }
                let multi = next.len() >= 2;
                if multi && (self.touch_pointer.is_some() || self.pressed.is_some()) {
                    let anchor = self
                        .touch_pointer
                        .and_then(|id| self.touches.get(&id).copied())
                        .or_else(|| next.values().next().copied())
                        .unwrap_or(Pos2::ZERO);
                    if let Some(button) = self.pressed.take() {
                        events.push(Event::PointerButton {
                            pos: anchor,
                            button,
                            pressed: false,
                            modifiers: self.modifiers,
                        });
                    }
                    if self.touch_pointer.take().is_some() {
                        events.push(Event::PointerGone);
                    }
                }
                let _first = self.touches.is_empty();
                for (&id, &p) in &next {
                    let phase = if self.touches.contains_key(&id) {
                        TouchPhase::Move
                    } else {
                        TouchPhase::Start
                    };
                    events.push(Event::Touch {
                        device_id: TouchDeviceId(0),
                        id: TouchId(id),
                        phase,
                        pos: p,
                        force: None,
                    });
                    if self.touch_pointer == Some(id) {
                        self.move_pointer(p, events);
                    }
                }
                // 单指视为主指针按下；多指只做导航（此处仅跟踪，不注入主指针）。
                if !multi && next.len() == 1 {
                    if let Some((&id, &p)) = next.iter().next() {
                        if self.touch_pointer.is_none() && self.pressed.is_none() {
                            self.touch_pointer = Some(id);
                            self.pressed = Some(PointerButton::Primary);
                            self.move_pointer(p, events);
                            events.push(Event::PointerButton {
                                pos: p,
                                button: PointerButton::Primary,
                                pressed: true,
                                modifiers: self.modifiers,
                            });
                        }
                    }
                }
                self.touches = next;
            }
            "pointer" => {
                let button = match input.button {
                    1 => PointerButton::Secondary,
                    2 => PointerButton::Middle,
                    _ => PointerButton::Primary,
                };
                self.move_pointer(pos, events);
                match input.action {
                    0 if self.pressed.is_none() => {
                        self.pressed = Some(button);
                        events.push(Event::PointerButton {
                            pos,
                            button,
                            pressed: true,
                            modifiers: self.modifiers,
                        });
                    }
                    1 => {
                        if let Some(b) = self.pressed.take() {
                            events.push(Event::PointerButton {
                                pos,
                                button: b,
                                pressed: false,
                                modifiers: self.modifiers,
                            });
                        }
                        // 同触摸：释放后不发 PointerGone，否则同一帧内清空 interact_pos，
                        // egui 的 click 判定失效（菜单栏/ribbon 点了没反应）。
                        self.last_pointer = Some(pos);
                    }
                    3 => self.cancel(events),
                    _ => {}
                }
            }
            "axis" => {
                // 桥在轴事件里带了指针位置（物理像素）。egui 和上游导航都靠
                // `pointer.hover_pos()` + `contains_pointer()` 判断"滚轮落在视口上"：
                // 不发 PointerMoved 就没有 hover，整条滚轮会被静默丢弃。这是滚轮
                // 缩放无效的真正原因（事件其实一直有到）。
                if input.x != 0.0 || input.y != 0.0 {
                    self.move_pointer(pos, events);
                }
                // 捏合比例 1.0（或 0）表示未缩放，别当成 zoom 事件把滚轮吞掉。
                if input.zoom.is_finite() && input.zoom > 0.0 && (input.zoom - 1.0).abs() > 1e-4 {
                    events.push(Event::Zoom(input.zoom));
                } else if input.dx != 0.0 || input.dy != 0.0 {
                    events.push(Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        phase: TouchPhase::Move,
                        // 滚动量是滚轮档位而不是坐标，不能除以 scale。上游按
                        // exp(-scroll * 0.0015) 缩放，标准一档约 50 点（≈7%），
                        // 而 OHOS 一档在这里约报 6 个单位，故乘 8 换算成点。
                        delta: egui::vec2(input.dx * WHEEL_POINTS_PER_UNIT, input.dy * WHEEL_POINTS_PER_UNIT),
                        modifiers: self.modifiers,
                    });
                }
            }
            "wheel" => events.push(Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: TouchPhase::Move,
                delta: egui::vec2(input.x, input.y),
                modifiers: self.modifiers,
            }),
            "key" => {
                let pressed = input.action == 0;
                if let Some(key) = key(input.code) {
                    events.push(Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: false,
                        modifiers: self.modifiers,
                    });
                }
                if pressed
                    && !input.ctrl
                    && !input.alt
                    && !input.text.is_empty()
                    && !input.text.chars().any(char::is_control)
                {
                    events.push(Event::Text(input.text));
                }
            }
            _ => {}
        }
    }
}

fn key(code: i32) -> Option<Key> {
    const LETTERS: [Key; 26] = [
        Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I, Key::J, Key::K, Key::L,
        Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X,
        Key::Y, Key::Z,
    ];
    const DIGITS: [Key; 10] = [
        Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8,
        Key::Num9,
    ];
    if (2017..=2042).contains(&code) {
        return LETTERS.get((code - 2017) as usize).copied();
    }
    if (2000..=2009).contains(&code) {
        return DIGITS.get((code - 2000) as usize).copied();
    }
    Some(match code {
        2012 => Key::ArrowUp,
        2013 => Key::ArrowDown,
        2014 => Key::ArrowLeft,
        2015 => Key::ArrowRight,
        2049 => Key::Tab,
        2050 => Key::Space,
        2054 => Key::Enter,
        2055 => Key::Backspace,
        2068 => Key::PageUp,
        2069 => Key::PageDown,
        2070 => Key::Escape,
        2071 => Key::Delete,
        2081 => Key::Home,
        2082 => Key::End,
        _ => return None,
    })
}
