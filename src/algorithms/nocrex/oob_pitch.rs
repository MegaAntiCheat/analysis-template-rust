// Written by Nocrex

use std::{collections::{HashMap, HashSet}};

use crate::{
    base::cheat_analyser_base::{CheatAnalyserState, PlayerState}
};

use anyhow::Error;
use serde_json::json;
use steamid_ng::SteamID;
use tf_demo_parser::{ParserState, demo::message::Message};

use crate::lib::algorithm::{CheatAlgorithm, Detection};
use crate::lib::parameters::{Parameter, Parameters, get_parameter_value};

pub struct OOBPitch {
    last_detections: HashSet<String>,
    server_name: String,
    
    params: Parameters,
}

impl OOBPitch {
    pub fn new() -> Self {
        let analyser: OOBPitch = OOBPitch {
            last_detections: HashSet::new(),
            server_name: "".to_string(),
            params: HashMap::from([
                ("min_pitch".to_string(), Parameter::Float(-89.999)),
                ("max_pitch".to_string(), Parameter::Float(89.999)),
            ]),
        };
        analyser
    }
}

impl<'a> CheatAlgorithm<'a> for OOBPitch {
    fn default(&self) -> bool {
        true
    }

    fn algorithm_name(&self) -> &str {
        "nocrex/oob_pitch"
    }

    fn handled_messages(&self) -> Result<Vec<tf_demo_parser::MessageType>, bool> {
        Ok(vec![tf_demo_parser::MessageType::ServerInfo, tf_demo_parser::MessageType::NetTick])
    }

    fn on_message(&mut self,
        message: &Message,
        state: &CheatAnalyserState,
        _: &ParserState,
        _: tf_demo_parser::demo::data::DemoTick) -> Result<Vec<Detection>, Error> {
        let mut submitted_detections = Vec::new();

        if let Message::ServerInfo(event) = message {
            if self.server_name == "" {
                self.server_name = event.server_name.trim().to_string();
            }
        }
        
        if let Message::NetTick(_) = message {
            let ticknum = u32::from(state.tick);
            let players = &state.players;

            let mut detections = HashSet::new();

            let min_pitch: f32 = get_parameter_value(&self.params, "min_pitch");
            let max_pitch: f32 = get_parameter_value(&self.params, "max_pitch");

            let is_valve_server = self.server_name.starts_with("Valve Matchmaking Server");

            for player in players.iter().filter(|p| {
                p.in_pvs
                    && p.state == PlayerState::Alive
                    && p.info.as_ref().is_some_and(|info| info.steam_id != "BOT")
            }) {
                let info = match &player.info {
                    Some(info) => info,
                    None => continue,
                };

                let steam_id = &info.steam_id;

                if !(min_pitch..=max_pitch).contains(&player.pitch_angle) {
                    detections.insert(steam_id.clone());
                    if !self.last_detections.contains(steam_id){
                        submitted_detections.push(Detection {
                            tick: ticknum,
                            algorithm: self.algorithm_name().to_string(),
                            player: u64::from(SteamID::from_steam3(&steam_id).unwrap()),
                            data: json!({
                                "pitch": player.pitch_angle,
                                "valve_server": is_valve_server
                            }),
                        });
                    }
                }
            }

            self.last_detections = detections;

        }

        Ok(submitted_detections)

    }
    
    fn params(&mut self) -> Option<&mut Parameters> {
        Some(&mut self.params)
    }
}
