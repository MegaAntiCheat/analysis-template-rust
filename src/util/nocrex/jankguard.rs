// Written by Nocrex

use std::collections::HashMap;

use crate::base::cheat_analyser_base::{CheatAnalyserState, Player, PlayerState};
use steamid_ng::SteamID;
use tf_demo_parser::demo::sendprop::SendPropIdentifier;

const TELEPORT_DIST: f32 = 256.0;

#[derive(Default)]
struct PlayerData {
    pub last_spawn: u32,
    pub last_teleport: u32,
    pub last_fire: u32,

    pub prev_state: Option<Player>,
}

#[derive(Default)]
pub struct JankGuard {
    player_data: HashMap<u64, PlayerData>,
}

impl JankGuard {
    pub fn teleported(&self, player: &u64, tick: u32) -> u32 {
        tick - self
            .player_data
            .get(player)
            .map_or(0, |pd| pd.last_teleport)
    }

    pub fn spawned(&self, player: &u64, tick: u32) -> u32 {
        tick - self.player_data.get(player).map_or(0, |pd| pd.last_spawn)
    }

    pub fn fired(&self, player: &u64, tick: u32) -> u32 {
        tick - self.player_data.get(player).map_or(0, |pd| pd.last_fire)
    }

    pub fn handled_messages(&self) -> Result<Vec<tf_demo_parser::MessageType>, bool> {
        Ok(vec![
            tf_demo_parser::MessageType::GameEvent,
            tf_demo_parser::MessageType::TempEntities,
        ])
    }

    pub fn on_message(
        &mut self,
        message: &tf_demo_parser::demo::message::Message,
        state: &CheatAnalyserState,
        parser_state: &tf_demo_parser::ParserState,
        tick: tf_demo_parser::demo::data::DemoTick,
    ) {
        match message {
            tf_demo_parser::demo::message::Message::GameEvent(
                tf_demo_parser::demo::message::GameEventMessage { event, .. },
            ) => match event {
                tf_demo_parser::demo::gamevent::GameEvent::PlayerSpawn(spawn) => {
                    if let Some(id) = state.get_id64_from_userid(spawn.user_id.into()) {
                        self.player_data.entry(id).or_default().last_spawn = tick.into();
                    }
                }
                tf_demo_parser::demo::gamevent::GameEvent::PostInventoryApplication(app) => {
                    if let Some(id) = state.get_id64_from_userid(app.user_id.into()) {
                        self.player_data.entry(id).or_default().last_spawn = tick.into();
                    }
                }
                tf_demo_parser::demo::gamevent::GameEvent::PlayerTeleported(tele) => {
                    if let Some(id) = state.get_id64_from_userid(tele.user_id.into()) {
                        self.player_data.entry(id).or_default().last_teleport = tick.into();
                    }
                }
                _ => (),
            },
            // Try to find firing events through tracers and player animations, simplified from megascatterbomb's snippet
            tf_demo_parser::demo::message::Message::TempEntities(msg) => {
                for event in &msg.events {
                    let class = &parser_state.server_classes[usize::from(event.class_id)].name;
                    if matches!(class.as_str(), "CTEFireBullets" | "CTEPlayerAnimEvent") {
                        const BULLETS_PLAYER: SendPropIdentifier =
                            SendPropIdentifier::new("DT_TEFireBullets", "m_iPlayer");
                        const ANIM_PLAYER: SendPropIdentifier =
                            SendPropIdentifier::new("DT_TEPlayerAnimEvent", "m_hPlayer");

                        if let Some(prop) = event
                            .props
                            .iter()
                            .find(|p| matches!(p.identifier, BULLETS_PLAYER | ANIM_PLAYER))
                        {
                            if let Some(id64) = i64::try_from(&prop.value)
                                .ok()
                                .and_then(|id| id.try_into().ok())
                                .map(|id|crate::util::helpers::handle_to_entid(id))
                                .and_then(|id| state.entid_to_userid.get(&id))
                                .and_then(|uid| state.userid_to_id64.get(uid))
                            {
                                self.player_data.entry(*id64).or_default().last_fire = tick.into();
                            }
                        }
                    }
                }
            }
            _ => (),
        }
    }

    pub fn on_tick(&mut self, state: &CheatAnalyserState) {
        let mut states = HashMap::new();
        for player in state.players.iter().filter(|p| {
            p.in_pvs
                && p.state == PlayerState::Alive
                && p.info.as_ref().is_some_and(|info| info.steam_id != "BOT")
        }) {
            let info = match &player.info {
                Some(info) => info,
                None => continue,
            };

            let steam_id: u64 = u64::from(SteamID::from_steam3(&info.steam_id).unwrap());

            let player_data = self.player_data.entry(steam_id).or_default();
            let prev_player = player_data.prev_state.as_ref();

            if prev_player.as_ref().is_some_and(|p| {
                // Ignore players that just moved more than 256 HUs in a single tick (teleport)
                let diff = p.position - player.position;
                let sq_len = diff.x.powi(2) + diff.y.powi(2) + diff.z.powi(2);
                sq_len > TELEPORT_DIST.powi(2)
            }) {
                player_data.last_teleport = state.tick.into();
            }
            states.insert(steam_id, player.clone());
        }
        for (steam_id, player_data) in self.player_data.iter_mut() {
            player_data.prev_state = states.remove(&steam_id);
        }
    }
}
