// Written by Tellta
use std::collections::HashMap;

use crate::base::cheat_analyser_base::CheatAnalyserState;

use crate::lib::algorithm::{CheatAlgorithm, Detection};
use crate::lib::parameters::{get_parameter_value, Parameter, Parameters};

use anyhow::Error;
use serde_json::{Map, Value};
use steamid_ng::SteamID;
use tf_demo_parser::demo::gameevent_gen::GameEvent;
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::ParserState;

#[derive(Default)]
pub struct DoubleTap {
    params: Parameters,

    shots: HashMap<u64, Vec<u32>>,
}

impl DoubleTap {
    pub fn new() -> Self {
        Self {
            params: HashMap::from([
                ("assert_pvs".to_string(), Parameter::Bool(true)),
                ("min_tick_scout".to_string(), Parameter::Int(17)),
                ("min_tick_heavy".to_string(), Parameter::Int(3)),
            ]),
            shots: HashMap::new(),
            ..Default::default()
        }
    }
}

fn is_cleaver_or_wrap_assassin(weapon_id: u32, damage: u32) -> bool {
    (weapon_id == 16 && damage <= 8 && damage >= 3) || (weapon_id == 16 && damage == 50)
}

impl<'a> CheatAlgorithm<'a> for DoubleTap {
    fn default(&self) -> bool {
        true
    }

    fn algorithm_name(&self) -> &str {
        "doubletap"
    }

    fn on_tick(
        &mut self,
        _: &CheatAnalyserState,
        _: &ParserState,
    ) -> Result<Vec<Detection>, Error> {
        Ok(vec![])
    }

    fn handled_messages(&self) -> Result<Vec<tf_demo_parser::MessageType>, bool> {
        Ok(vec![tf_demo_parser::MessageType::GameEvent])
    }

    fn on_message(
        &mut self,
        message: &tf_demo_parser::demo::message::Message,
        state: &CheatAnalyserState,
        _: &ParserState,
        tick: tf_demo_parser::demo::data::DemoTick,
    ) -> Result<Vec<Detection>, Error> {
        let mut detections = Vec::new();
        let ticknum = u32::from(tick);
        let players = &state.players;

        if let Message::GameEvent(event_msg) = message {
            if let GameEvent::PlayerHurt(hurt) = &event_msg.event {
                let assert_pvs: bool = get_parameter_value(&self.params, "assert_pvs");
                let min_tick_scout: i32 = get_parameter_value(&self.params, "min_tick_scout");
                let min_tick_heavy: i32 = get_parameter_value(&self.params, "min_tick_heavy");

                // format;
                // weapon id, min ticks
                let weapon_mapping = HashMap::from([
                    (16, min_tick_scout as u32), // scout primary
                    (18, min_tick_heavy as u32), // heavy primary
                ]);

                let dmg = hurt.damage_amount as u32;
                let weapon = hurt.weapon_id as u32;

                if is_cleaver_or_wrap_assassin(weapon, dmg) {
                    // ignore wrap assassin bleed
                    return Ok(vec![]);
                }

                let attacker_uid = u32::from(hurt.attacker);
                let victim_uid = u32::from(hurt.user_id);

                // get steam id64 from uids
                let mut attacker_sid = 0;
                let mut victim_sid = 0;
                for player in &state.players {
                    if let Some(info) = &player.info {
                        if u32::from(info.user_id) == attacker_uid {
                            attacker_sid =
                                u64::from(SteamID::from_steam3(&info.steam_id).unwrap_or_default());
                        } else if u32::from(info.user_id) == victim_uid {
                            victim_sid =
                                u64::from(SteamID::from_steam3(&info.steam_id).unwrap_or_default());
                        }
                    }
                }
                if attacker_sid == 0 || victim_sid == 0 {
                    return Ok(vec![]);
                }

                let attacker = match players.iter().find(|x| {
                    x.info.as_ref().is_some_and(|info| {
                        u64::from(SteamID::from_steam3(&info.steam_id).unwrap_or_default())
                            == attacker_sid
                    })
                }) {
                    Some(p) => p,
                    None => return Ok(vec![]),
                };

                if assert_pvs && !attacker.in_pvs {
                    return Ok(vec![]);
                }

                let shots = self.shots.entry(attacker_sid).or_default();

                let past_shot = shots.clone();

                shots.clear();
                shots.extend([ticknum, weapon, victim_uid, dmg]);

                if past_shot.len() < 2 {
                    return Ok(vec![]);
                }

                let past_shot: [u32; 4] = match past_shot.try_into() {
                    Ok(arr) => arr,
                    Err(_) => return Ok(vec![]), // or handle error
                };

                let [past_tick, past_weapon, past_victim, past_dmg] = past_shot;
                let Some(&min_diff) = weapon_mapping.get(&weapon) else {
                    return Ok(vec![]);
                };

                if past_weapon != weapon || past_victim != victim_uid {
                    return Ok(vec![]);
                }

                let diff = ticknum - past_tick;

                if diff < min_diff && diff > 0 {
                    let u200b = "​";
                    let data: Vec<(&str, Value)> = vec![
                        ("class", Value::from(attacker.class.to_string())),
                        ("tick_1", Value::from(past_tick)),
                        ("tick_2", Value::from(ticknum)),
                        ("tick_diff", Value::from(diff)),
                        ("victim", Value::from(victim_uid)),
                        ("weapon_id", Value::from(weapon)),
                        ("damage_1", Value::from(past_dmg)),
                        ("damage_2", Value::from(dmg)),
                    ];
                    let mut new_data = Map::new();
                    for (i, (key, value)) in data.into_iter().enumerate() {
                        new_data.insert(format!("{}{}", u200b.repeat(i), key), value);
                    }
                    let new_data = Value::Object(new_data);

                    detections.push(Detection {
                        tick: tick.into(),
                        algorithm: self.algorithm_name().to_string(),
                        player: attacker_sid,
                        data: new_data,
                    });
                }
            }
        }
        Ok(detections)
    }

    fn finish(&mut self) -> Result<Vec<Detection>, Error> {
        Ok(vec![])
    }

    fn params(&mut self) -> Option<&mut Parameters> {
        Some(&mut self.params)
    }
}
