// Written by Tellta
use std::collections::HashMap;

use crate::{
    base::cheat_analyser_base::{CheatAnalyserState, Player, PlayerState}, util::{helpers::{angle_delta}, nocrex::jankguard::JankGuard}
};

use crate::lib::algorithm::{CheatAlgorithm, Detection};
use crate::lib::parameters::{Parameter, Parameters, get_parameter_value};

use anyhow::Error;
use serde_json::json;
use steamid_ng::SteamID;
use tf_demo_parser::ParserState;

#[derive(Default)]
pub struct AngleHistory {
    ticks: Vec<HashMap<u64, Player>>,

    jg: JankGuard,
    params: Parameters,
    detections: Vec<Detection>,
}

impl AngleHistory {
    pub fn new() -> Self {
        Self {
            params: HashMap::from([
                ("tick_window".to_string(), Parameter::Int(4)),
                ("max_delta_first_third".to_string(), Parameter::Float(0.5)),
                ("min_delta_second_third".to_string(), Parameter::Float(10.0)),
            ]),
            ..Default::default()
        }
    }
}

impl<'a> CheatAlgorithm<'a> for AngleHistory {
    fn default(&self) -> bool {
        true
    }

    fn algorithm_name(&self) -> &str {
        "angle_history"
    }

    fn on_tick(
        &mut self,
        state: &CheatAnalyserState,
        _: &ParserState,
    ) -> Result<Vec<Detection>, Error> {
        self.jg.on_tick(state);
        let ticknum = u32::from(state.tick);
        let players = &state.players;
        
        let tick_window: i32 = get_parameter_value(&self.params, "tick_window");
        let max_delta_first_third: f32 = get_parameter_value(&self.params, "max_delta_first_third");
        let min_delta_second_third: f32 = get_parameter_value(&self.params, "min_delta_second_third");

        self.ticks.insert(0, HashMap::new());
        self.ticks.truncate(tick_window as usize);

        for player in players.iter().filter(|p| {
            p.in_pvs
            && p.state == PlayerState::Alive
            && p.info.as_ref().is_some_and(|info| info.steam_id != "BOT")
        }) {

            let info = match &player.info {
                Some(info) => info,
                None => continue,
            };

            let steam_id: u64 = u64::from(SteamID::from_steam3(&info.steam_id).unwrap());

            let ticks_since_event = self
                .jg
                .teleported(&steam_id, ticknum)
                .min(self.jg.spawned(&steam_id, ticknum));

            if ticks_since_event < 60 {
                if ticks_since_event == 0 {
                    self.detections
                        .retain(|det| det.player != steam_id || (ticknum - det.tick) > 60);
                }
                continue;
            }

            self.ticks
                .get_mut(0)
                .unwrap()
                .insert(steam_id, player.clone());

            let current_angle = (player.view_angle, player.pitch_angle);

            let mut match_index: Option<usize> = None;
            let mut delta_one = 0.0;

            for i in 1..self.ticks.len() {

                let past_player = match self.ticks.get(i).and_then(|m| m.get(&steam_id)) {
                    Some(p) => p,
                    None => continue,
                };

                if !(past_player.in_pvs && past_player.state == PlayerState::Alive) {
                    continue;
                }

                let past_angle = (past_player.view_angle, past_player.pitch_angle);
                let delta = angle_delta(current_angle, past_angle);

                if delta < max_delta_first_third {
                    delta_one = delta;
                    match_index = Some(i);
                    break;
                }
            }

            if let Some(i) = match_index {

                let mid = (1 + i) / 2;
                let mut mids = vec![mid];

                if (i - 1) % 2 != 0 {
                    mids.push(mid + 1);
                }

                for m in &mids {

                    if let Some(mid_player) = self.ticks.get(*m).and_then(|map| map.get(&steam_id)) {

                        let mid_angle = (mid_player.view_angle, mid_player.pitch_angle);
                        let mid_delta = angle_delta(current_angle, mid_angle);

                        if mid_delta > min_delta_second_third 
                            && self.jg.fired(&steam_id, ticknum) <= (i as u32 + 5) {

                            self.detections.push(Detection {
                                tick: ticknum,
                                algorithm: self.algorithm_name().to_string(),
                                player: steam_id,
                                data: json!({
                                    "angle_current": current_angle,
                                    "angle_middle": (mid_player.view_angle, mid_player.pitch_angle),
                                    "angle_trigger": (player.view_angle, player.pitch_angle),
                                    "delta_1_3": delta_one,
                                    "delta_2_3": mid_delta,
                                    "match_index": i,
                                    "middle_indices": mids,
                                    "middle_trigger": m,
                                }),
                            });
                        }
                    }
                }
            }
        }
        Ok(vec![])
    }

    fn handled_messages(&self) -> Result<Vec<tf_demo_parser::MessageType>, bool> {
        self.jg.handled_messages()
    }

    fn on_message(
        &mut self,
        message: &tf_demo_parser::demo::message::Message,
        state: &CheatAnalyserState,
        parser_state: &ParserState,
        tick: tf_demo_parser::demo::data::DemoTick,
    ) -> Result<Vec<Detection>, Error> {
        self.jg.on_message(message, state, parser_state, tick);
        Ok(vec![])
    }

    fn finish(&mut self) -> Result<Vec<Detection>, Error> {
        Ok(self.detections.clone())
    }

    fn params(&mut self) -> Option<&mut Parameters> {
        Some(&mut self.params)
    }
}
