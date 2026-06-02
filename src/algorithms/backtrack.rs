// Written by Tellta
use std::{collections::HashMap};

use crate::{base::cheat_analyser_base::{CheatAnalyserState}};

use crate::lib::algorithm::{CheatAlgorithm, Detection};
use crate::lib::parameters::{Parameter, Parameters, get_parameter_value};

use anyhow::Error;
use itertools::any;
use serde_json::{Map, Value};
use steamid_ng::SteamID;
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::gameevent_gen::GameEvent;
use tf_demo_parser::ParserState;

#[derive(Default)]
pub struct BackTrack {
    params: Parameters,
}

impl BackTrack {
    pub fn new() -> Self {
        Self {
            params: HashMap::from([
                ("distance".to_string(), Parameter::Float(200.0)),
                ("max_distance".to_string(), Parameter::Float(600.0)),
                ("max_angle_diff".to_string(), Parameter::Float(110.0)),
            ]),
            ..Default::default()
        }
    }
}

fn angle_diff(a: f32, b: f32) -> f32 {
    (b - a + 180.0).rem_euclid(360.0) - 180.0
}

fn is_backstab(damage: u16, is_crit: bool, weapon_id: u16, victim_health: u16) -> bool {
    return is_crit && weapon_id == 7 && (damage > 600 || damage as f32 > victim_health as f32 * 5.5);
}

impl<'a> CheatAlgorithm<'a> for BackTrack {
    fn default(&self) -> bool {
        true
    }

    fn algorithm_name(&self) -> &str {
        "backtrack"
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

        if let Message::GameEvent(event_msg) = message {
            if let GameEvent::PlayerHurt(hurt) = &event_msg.event {
                let victim_uid = u32::from(hurt.user_id);
                let attacker_uid = u32::from(hurt.attacker);
                
                let is_crit = hurt.crit;

                let weapon_id = hurt.weapon_id as u16;
                let damage_amount = hurt.damage_amount;

                let distance: f32 = get_parameter_value(&self.params, "distance");
                let max_distance: f32 = get_parameter_value(&self.params, "max_distance");
                let max_angle_diff: f32 = get_parameter_value(&self.params, "max_angle_diff");

                // get steam id64 from uids
                let mut attacker_sid = 0;
                let mut victim_sid = 0;
                for player in &state.players {
                    if let Some(info) = &player.info {
                        if u32::from(info.user_id) == attacker_uid { attacker_sid = u64::from(SteamID::from_steam3(&info.steam_id).unwrap_or_default());
                        } if u32::from(info.user_id) == victim_uid { victim_sid = u64::from(SteamID::from_steam3(&info.steam_id).unwrap_or_default());
                        }
                    }
                }
                if any([attacker_sid, victim_sid], |x| x == 0) {
                    return Ok(vec![]);
                }

                let players = &state.players;

                let attacker = match players.iter().find(|x| x.info.as_ref().is_some_and(|info| u64::from(SteamID::from_steam3(&info.steam_id).unwrap_or_default()) == attacker_sid )) {
                    Some(p) => p,
                    None => return Ok(vec![])
                };
                let attacker_pos = attacker.position;

                let victim = match players.iter().find(|x| x.info.as_ref().is_some_and(|info| u64::from(SteamID::from_steam3(&info.steam_id).unwrap_or_default()) == victim_sid )) {
                    Some(p) => p,
                    None => return Ok(vec![])
                };
                let victim_pos = victim.position;
                let victim_health = victim.health;

                if !attacker.in_pvs || !victim.in_pvs {
                    return Ok(vec![]);
                }
                
                if attacker_uid != victim_uid && is_backstab(damage_amount, is_crit, weapon_id, victim_health) {

                    let angle_diff = angle_diff(attacker.view_angle, victim.view_angle).abs();

                    // 3d distance
                    // let pos_diff = (f32::powi(attacker_pos.x - victim_pos.x, 2) +
                    //                     f32::powi(attacker_pos.y - victim_pos.y, 2) +
                    //                     f32::powi(attacker_pos.z - victim_pos.z, 2)).sqrt();

                    // 2d distance
                    let pos_diff = (f32::powi(attacker_pos.x - victim_pos.x, 2) +
                                        f32::powi(attacker_pos.y - victim_pos.y, 2)).sqrt();
                            

                    

                    

                    if (pos_diff > distance && pos_diff < max_distance) || (angle_diff > max_angle_diff && pos_diff < max_distance) {

                        // let data = json!({
                        //     "angle_attacker": attacker.view_angle,
                        //     "angle_victim": victim.view_angle,
                        //     "angle_diff": angle_diff,
                        //     "pos_attacker": attacker_pos,
                        //     "pos_victim": victim_pos,
                        //     "distance": pos_diff,
                        //     "damage": damage_amount,
                        //     "type": if angle_diff > max_angle_diff && pos_diff < max_distance {"Angle"} else {"Distance"},
                        //     "victim_class": victim.class,
                        //     "victim_health": victim_health,
                        // });

                        // ########################

                        let u200b = "​";
                        let data: Vec<(&str, Value)> = vec![
                            ("angle_attacker", Value::from(attacker.view_angle)),
                            ("angle_victim", Value::from(victim.view_angle)),
                            ("angle_diff", Value::from(angle_diff)),
                            ("pos_attacker", Value::from(vec![attacker_pos.x,attacker_pos.y,attacker_pos.z])),
                            ("pos_victim", Value::from(vec![victim_pos.x,victim_pos.y,victim_pos.z])),
                            ("distance", Value::from(pos_diff)),
                            ("damage", Value::from(damage_amount)),
                            ("type", Value::from(if angle_diff > max_angle_diff && pos_diff < max_distance {"Angle" } else { "Distance"})),
                            ("victim_class", Value::from(victim.class.to_string())),
                            ("victim_health", Value::from(victim.health)),
                        ];
                        let mut new_data = Map::new();
                        for (i, (key, value)) in data.into_iter().enumerate() { new_data.insert(format!("{}{}", u200b.repeat(i), key), value); }
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
