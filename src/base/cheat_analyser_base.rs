// This file is a modified version of GameStateAnalyser.
// https://github.com/demostf/parser/blob/master/src/demo/parser/gamestateanalyser.rs
// TODO: This version will add support for sub analysers that can be used to extend functionality as needed
// without creating an entirely separate analyser.
// Additional functionality that has broad utility can be merged into this base analyser.

use anyhow::Error;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::str::FromStr;
use std::sync::Mutex;
use steamid_ng::SteamID;
use tf_demo_parser::demo::data::DemoTick;
use tf_demo_parser::demo::gameevent_gen::ObjectDestroyedEvent;
use tf_demo_parser::demo::gamevent::GameEvent;
use tf_demo_parser::demo::header::Header;
use tf_demo_parser::demo::message::gameevent::GameEventMessage;
use tf_demo_parser::demo::message::packetentities::{EntityId, PacketEntity, UpdateType};
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::packet::datatable::{ParseSendTable, ServerClass, ServerClassName};
use tf_demo_parser::demo::packet::message::MessagePacketMeta;
use tf_demo_parser::demo::packet::stringtable::StringTableEntry;
use tf_demo_parser::demo::parser::analyser::UserInfo;
pub use tf_demo_parser::demo::parser::analyser::{Class, Team, UserId};
use tf_demo_parser::demo::parser::handler::BorrowMessageHandler;
use tf_demo_parser::demo::parser::MessageHandler;
use tf_demo_parser::demo::sendprop::{SendProp, SendPropIdentifier, SendPropValue};
use tf_demo_parser::demo::vector::{Vector, VectorXY};
use tf_demo_parser::{MessageType, ParserState, ReadResult, Stream};
use web_time::Instant;

use crate::lib::algorithm::{CheatAlgorithm, Detection};
use crate::dev_print;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum PlayerState {
    #[default]
    Alive = 0,
    Dying = 1,
    Death = 2,
    Respawnable = 3,
}

impl PlayerState {
    pub fn new(number: i64) -> Self {
        match number {
            1 => PlayerState::Dying,
            2 => PlayerState::Death,
            3 => PlayerState::Respawnable,
            _ => PlayerState::Alive,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Player {
    entity: EntityId,
    pub position: Vector,
    pub health: u16,
    pub max_health: u16,
    pub class: Class,
    pub team: Team,
    pub view_angle: f32,
    pub pitch_angle: f32,
    pub state: PlayerState,
    pub info: Option<UserInfo>,
    pub charge: u8,
    pub simtime: u16,
    pub ping: u16,
    pub in_pvs: bool,
    // pub shot_fired: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Sentry {
    pub entity: EntityId,
    pub builder: UserId,
    pub position: Vector,
    pub level: u8,
    pub max_health: u16,
    pub health: u16,
    pub building: bool,
    pub sapped: bool,
    pub team: Team,
    pub angle: f32,
    pub player_controlled: bool,
    pub auto_aim_target: UserId,
    pub shells: u16,
    pub rockets: u16,
    pub is_mini: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Dispenser {
    pub entity: EntityId,
    pub builder: UserId,
    pub position: Vector,
    pub level: u8,
    pub max_health: u16,
    pub health: u16,
    pub building: bool,
    pub sapped: bool,
    pub team: Team,
    pub angle: f32,
    pub healing: Vec<UserId>,
    pub metal: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Teleporter {
    pub entity: EntityId,
    pub builder: UserId,
    pub position: Vector,
    pub level: u8,
    pub max_health: u16,
    pub health: u16,
    pub building: bool,
    pub sapped: bool,
    pub team: Team,
    pub angle: f32,
    pub is_entrance: bool,
    pub other_end: EntityId,
    pub recharge_time: f32,
    pub recharge_duration: f32,
    pub times_used: u16,
    pub yaw_to_exit: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Building {
    Sentry(Sentry),
    Dispenser(Dispenser),
    Teleporter(Teleporter),
}

impl Building {
    pub fn new(entity_id: EntityId, class: BuildingClass) -> Building {
        match class {
            BuildingClass::Sentry => Building::Sentry(Sentry {
                entity: entity_id,
                ..Sentry::default()
            }),
            BuildingClass::Dispenser => Building::Dispenser(Dispenser {
                entity: entity_id,
                ..Dispenser::default()
            }),
            BuildingClass::Teleporter => Building::Teleporter(Teleporter {
                entity: entity_id,
                ..Teleporter::default()
            }),
        }
    }

    pub fn entity_id(&self) -> EntityId {
        match self {
            Building::Sentry(Sentry { entity, .. })
            | Building::Dispenser(Dispenser { entity, .. })
            | Building::Teleporter(Teleporter { entity, .. }) => *entity,
        }
    }

    pub fn level(&self) -> u8 {
        match self {
            Building::Sentry(Sentry { level, .. })
            | Building::Dispenser(Dispenser { level, .. })
            | Building::Teleporter(Teleporter { level, .. }) => *level,
        }
    }

    pub fn position(&self) -> Vector {
        match self {
            Building::Sentry(Sentry { position, .. })
            | Building::Dispenser(Dispenser { position, .. })
            | Building::Teleporter(Teleporter { position, .. }) => *position,
        }
    }

    pub fn builder(&self) -> UserId {
        match self {
            Building::Sentry(Sentry { builder, .. })
            | Building::Dispenser(Dispenser { builder, .. })
            | Building::Teleporter(Teleporter { builder, .. }) => *builder,
        }
    }

    pub fn angle(&self) -> f32 {
        match self {
            Building::Sentry(Sentry { angle, .. })
            | Building::Dispenser(Dispenser { angle, .. })
            | Building::Teleporter(Teleporter { angle, .. }) => *angle,
        }
    }

    pub fn max_health(&self) -> u16 {
        match self {
            Building::Sentry(Sentry { max_health, .. })
            | Building::Dispenser(Dispenser { max_health, .. })
            | Building::Teleporter(Teleporter { max_health, .. }) => *max_health,
        }
    }

    pub fn health(&self) -> u16 {
        match self {
            Building::Sentry(Sentry { health, .. })
            | Building::Dispenser(Dispenser { health, .. })
            | Building::Teleporter(Teleporter { health, .. }) => *health,
        }
    }

    pub fn sapped(&self) -> bool {
        match self {
            Building::Sentry(Sentry { sapped, .. })
            | Building::Dispenser(Dispenser { sapped, .. })
            | Building::Teleporter(Teleporter { sapped, .. }) => *sapped,
        }
    }

    pub fn team(&self) -> Team {
        match self {
            Building::Sentry(Sentry { team, .. })
            | Building::Dispenser(Dispenser { team, .. })
            | Building::Teleporter(Teleporter { team, .. }) => *team,
        }
    }

    pub fn class(&self) -> BuildingClass {
        match self {
            Building::Sentry(_) => BuildingClass::Sentry,
            Building::Dispenser(_) => BuildingClass::Sentry,
            Building::Teleporter(_) => BuildingClass::Teleporter,
        }
    }
}

pub enum BuildingClass {
    Sentry,
    Dispenser,
    Teleporter,
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct World {
    pub boundary_min: Vector,
    pub boundary_max: Vector,
}

// #[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
// pub struct Kill {
//     pub attacker_id: u16,
//     pub assister_id: u16,
//     pub victim_id: u16,
//     pub weapon: String,
//     pub tick: DemoTick,
// }

// impl Kill {
//     fn new(tick: DemoTick, death: &PlayerDeathEvent) -> Self {
//         Kill {
//             attacker_id: death.attacker,
//             assister_id: death.assister,
//             victim_id: death.user_id,
//             weapon: death.weapon.to_string(),
//             tick,
//         }
//     }
// }

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct CheatAnalyserState {
    pub players: Vec<Player>,
    pub entid_to_userid: HashMap<EntityId, UserId>,
    pub userid_to_id64: HashMap<UserId, u64>,
    pub buildings: BTreeMap<EntityId, Building>,
    pub world: Option<World>,
    // pub kills: Vec<Kill>,
    pub tick: DemoTick,
}

impl CheatAnalyserState {
    pub fn get_or_create_player(&mut self, entity_id: EntityId) -> &mut Player {
        let index = match self
            .players
            .iter()
            .enumerate()
            .find(|(_index, player)| player.entity == entity_id)
            .map(|(index, _)| index)
        {
            Some(index) => index,
            None => {
                let index = self.players.len();
                self.players.push(Player {
                    entity: entity_id,
                    ..Player::default()
                });
                index
            }
        };

        #[allow(clippy::indexing_slicing)]
        &mut self.players[index]
    }
    pub fn get_userid_from_entid(&self, entid: EntityId) -> Option<UserId> {
        self.entid_to_userid.get(&entid).copied()
    }

    pub fn get_id64_from_userid(&self, userid: UserId) -> Option<u64> {
        self.userid_to_id64.get(&userid).copied()
    }

    pub fn set_entid_to_userid(&mut self, entid: EntityId, userid: UserId) {
        self.entid_to_userid.insert(entid, userid);
    }

    pub fn set_userid_to_id64(&mut self, userid: UserId, id64: u64) {
        self.userid_to_id64.insert(userid, id64);
    }

    pub fn get_or_create_building(
        &mut self,
        entity_id: EntityId,
        class: BuildingClass,
    ) -> &mut Building {
        self.buildings
            .entry(entity_id)
            .or_insert_with(|| Building::new(entity_id, class))
    }

    pub fn remove_building(&mut self, entity_id: EntityId) {
        self.buildings.remove(&entity_id);
    }
}

// ParserState requires a non-self impl of does_handle so I had to create this.
lazy_static! {
    static ref HANDLED_MESSAGE_TYPES: Mutex<Vec<MessageType>> = Mutex::new(vec![
        MessageType::PacketEntities,
        MessageType::GameEvent,
        MessageType::NetTick
    ]);
}

pub struct CheatAnalyser<'a> {
    pub state: CheatAnalyserState,
    pub algorithms: Vec<Box<dyn CheatAlgorithm<'a> + 'a + Send>>,
    pub detections: Vec<Detection>,
    pub header: Option<Header>,
    pub tick: DemoTick,
    last_progress_update_time: Instant,
    progress: Vec<u32>,
    class_names: Vec<ServerClassName>, // indexed by ClassId
}

impl<'a> Default for CheatAnalyser<'a> {
    fn default() -> Self {
        Self {
            state: Default::default(),
            algorithms: Default::default(),
            detections: Default::default(),
            header: Default::default(),
            tick: Default::default(),
            last_progress_update_time: Instant::now(),
            progress: Default::default(),
            class_names: Default::default(),
        }
    }
}

impl MessageHandler for CheatAnalyser<'_> {
    type Output = CheatAnalyserState;

    fn does_handle(message_type: MessageType) -> bool {
        let message_types = HANDLED_MESSAGE_TYPES.lock().unwrap();
        message_types.is_empty() || message_types.contains(&message_type)
    }

    fn handle_header(&mut self, _header: &tf_demo_parser::demo::header::Header) {
        self.header = Some(_header.clone());
        self.print_metadata();
    }

    fn handle_message(&mut self, message: &Message, _tick: DemoTick, parser_state: &ParserState) {
        match message {
            Message::PacketEntities(message) => {
                for entity in &message.entities {
                    self.handle_entity(entity, parser_state);
                }
            }
            Message::NetTick(_) => {
                self.check_progress();
                for algorithm in &mut self.algorithms {
                    match algorithm.on_tick(&self.state, parser_state) {
                        Ok(detections) => self.detections.extend(detections),
                        Err(_) => {}
                    }
                }
            }
            Message::TempEntities(_) => {
                // println!("{}: {:#?}", _tick, message);
            }
            Message::GameEvent(GameEventMessage { event, .. }) => match event {
                // GameEvent::PlayerDeath(death) => {
                //     self.state.kills.push(Kill::new(self.tick, death.as_ref()))
                // }
                // TODO: Wait for https://github.com/demostf/parser/issues/25 to be resolved
                // GameEvent::PlayerShoot(_) => {
                //     println!("player shoot event");
                //     // let player = self.state.players.iter_mut().find(|p|{
                //     //     p.info.as_ref().is_some_and(|info| {
                //     //         println!("{} == {}", info.user_id, user_id);
                //     //         info.user_id == *user_id
                //     //     })
                //     // });
                //     // if let Some(player) = player {
                //     //     player.shot_fired = u32::from(_tick);
                //     // }
                // }
                GameEvent::RoundStart(_) => {
                    self.state.buildings.clear();
                }
                GameEvent::TeamPlayRoundStart(_) => {
                    self.state.buildings.clear();
                }
                GameEvent::ObjectDestroyed(ObjectDestroyedEvent { index, .. }) => {
                    self.state.remove_building((*index as u32).into());
                }
                GameEvent::PlayerConnectClient(event) => {
                    self.state.set_entid_to_userid(
                        EntityId::from(event.index as u32),
                        UserId::from(event.user_id),
                    );
                    if event.network_id != "BOT".into() {
                        let steamid = SteamID::from_steam3(event.network_id.to_string().as_str());
                        let steamid64 = u64::from(steamid.unwrap_or(0.into()));
                        self.state
                            .set_userid_to_id64(event.user_id.into(), steamid64);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        for algorithm in &mut self.algorithms {
            if !algorithm.does_handle(message.get_message_type()) {
                continue;
            }
            match algorithm.on_message(message, &self.state, &parser_state, _tick) {
                Ok(detections) => self.detections.extend(detections),
                Err(_) => {}
            }
        }
    }

    fn handle_string_entry(
        &mut self,
        table: &str,
        index: usize,
        entry: &StringTableEntry,
        _parser_state: &ParserState,
    ) {
        if table == "userinfo" {
            let _ = self.parse_user_info(
                index,
                entry.text.as_ref().map(|s| s.as_ref()),
                entry.extra_data.as_ref().map(|data| data.data.clone()),
            );
        }
    }

    fn handle_data_tables(
        &mut self,
        _parse_tables: &[ParseSendTable],
        server_classes: &[ServerClass],
        _parser_state: &ParserState,
    ) {
        self.class_names = server_classes
            .iter()
            .map(|class| &class.name)
            .cloned()
            .collect();
    }

    fn handle_packet_meta(
        &mut self,
        tick: DemoTick,
        _meta: &MessagePacketMeta,
        _parser_state: &ParserState,
    ) {
        self.state.tick = tick;
        self.tick = tick;
    }

    fn into_output(self, _state: &ParserState) -> Self::Output {
        self.state
    }
}

impl BorrowMessageHandler for CheatAnalyser<'_> {
    fn borrow_output(&self, _state: &ParserState) -> &Self::Output {
        &self.state
    }
}

impl<'a> CheatAnalyser<'a> {
    pub fn new(algorithms: Vec<Box<dyn CheatAlgorithm<'a> + 'a + Send>>) -> Self {
        let mut message_types = HANDLED_MESSAGE_TYPES.lock().unwrap();
        // Figure out what message types we're going to be using.
        let mut specified_message_types: Vec<MessageType> = vec![];
        for algorithm in &algorithms {
            match algorithm.handled_messages() {
                Ok(types) => specified_message_types.extend(types),
                Err(true) => {
                    // An empty HANDLED_MESSAGE_TYPES means we parse ALL messages.
                    message_types.clear();
                    break;
                }
                Err(false) => {}
            }
        }

        if !message_types.is_empty() {
            message_types.extend(specified_message_types);
            message_types.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
            message_types.dedup();
        }

        Self {
            state: Default::default(),
            algorithms,
            detections: Vec::new(),
            header: None,
            tick: DemoTick::default(),
            last_progress_update_time: Instant::now(),
            progress: vec![],
            class_names: Vec::new(),
        }
    }

    pub fn init(&mut self) -> Result<(), Error> {
        for algorithm in &mut self.algorithms {
            match algorithm.init() {
                Ok(_) => {}
                Err(_) => continue,
            }
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), Error> {
        for algorithm in &mut self.algorithms {
            match algorithm.finish() {
                Ok(detections) => self.detections.extend(detections),
                Err(_) => continue,
            }
        }
        Ok(())
    }

    pub fn print_metadata(&self) {
        if self.header.is_none() {
            return;
        }
        let header = self.header.as_ref().unwrap();
        let ticks = self.get_tick_count_u32();

        dev_print!("Map: {}", header.map);
        let hours = (header.duration / 3600.0).floor();
        let minutes = ((header.duration % 3600.0) / 60.0).floor();
        let seconds = (header.duration % 60.0).floor();
        let milliseconds = ((header.duration % 1.0) * 100.0).floor();
        dev_print!(
            "Duration: {:02}:{:02}:{:02}.{:03} ({} ticks)",
            hours,
            minutes,
            seconds,
            milliseconds,
            ticks
        );
        dev_print!("User: {}", header.nick);
        dev_print!("Server: {}", header.server);
    }

    pub fn print_detection_json(&self, pretty: bool) {
        let analysis = serde_json::json!({
            "server_ip": self.header.as_ref().map_or("unknown".to_string(), |h| h.server.clone()),
            "duration": self.tick,
            "author": self.header.as_ref().map_or("unknown".to_string(), |h| h.nick.clone()),
            "map": self.header.as_ref().map_or("unknown".to_string(), |h| h.map.clone()),
            "detections": self.detections
        });
        let json = if pretty {
            serde_json::to_string_pretty(&analysis).unwrap()
        } else {
            serde_json::to_string(&analysis).unwrap()
        };
        println!("{}", json);
    }

    pub fn print_detection_summary(&self) {
        let mut algorithm_counts: HashMap<String, HashMap<u64, usize>> = HashMap::new();
        for detection in &self.detections {
            let algorithm = detection.algorithm.clone();
            let steamid = detection.player;
            *algorithm_counts
                .entry(algorithm)
                .or_insert(HashMap::new())
                .entry(steamid)
                .or_insert(0) += 1;
        }

        dev_print!("Total detections: {}", self.detections.len());
        if self.detections.is_empty() {
            return;
        }
        dev_print!("Detections by Algorithm:");
        for (algorithm, steamid_counts) in algorithm_counts {
            dev_print!(
                "  {}: {} players, {} detections",
                algorithm,
                steamid_counts.len(),
                steamid_counts.values().sum::<usize>()
            );
            let mut steamid_counts_vec: Vec<_> = steamid_counts.into_iter().collect();
            steamid_counts_vec.sort_by(|a, b| b.1.cmp(&a.1));
            for (steamid, count) in steamid_counts_vec {
                dev_print!("    {}: {}", steamid, count);
            }
        }
    }
    // This code doesn't include the very first interval in any averages.
    // I didn't intend for that but it makes sense to exclude the intitial interval since
    // there tends to be a lot of boiler plate stuff which throws off the average anyway.
    fn check_progress(&mut self) {
        const PROGRESS_UPDATE_INTERVAL_MS: u128 = 1000;
        const TPS_ROLLING_AVERAGE_WINDOW: u32 = 10;
        crate::PROGRESS_CURRENT.store(self.tick.into(), std::sync::atomic::Ordering::Relaxed);
        crate::PROGRESS_TOTAL.store(self.get_tick_count_u32(), std::sync::atomic::Ordering::Relaxed);
        if self.last_progress_update_time.elapsed().as_millis() < PROGRESS_UPDATE_INTERVAL_MS {
            return;
        }
        let tick: u32 = self.tick.into();

        self.last_progress_update_time = Instant::now();
        self.progress.push(tick);
        while self.progress.len() > TPS_ROLLING_AVERAGE_WINDOW.try_into().unwrap() {
            self.progress.remove(0);
        }

        let tps = if self.progress.len() >= 2 {
            let tps = (self.progress.last().unwrap() - self.progress.first().unwrap()) as f64
                / (self.progress.len() as f64 - 1.0);
            tps * PROGRESS_UPDATE_INTERVAL_MS as f64 / 1000.0
        } else {
            tick.into()
        };

        dev_print!(
            "Processing tick {} ({} remaining, {:.0} tps)",
            tick,
            self.get_tick_count_u32() - tick,
            tps
        );
    }

    pub fn get_tick_count_u32(&self) -> u32 {
        if self.header.is_none() {
            return self.tick.into();
        }
        let header = self.header.as_ref().unwrap();
        if self.tick > header.ticks {
            self.tick.into()
        } else {
            header.ticks
        }
    }

    pub fn handle_entity(&mut self, entity: &PacketEntity, parser_state: &ParserState) {
        let class_name: &str = self
            .class_names
            .get(usize::from(entity.server_class))
            .map(|class_name| class_name.as_str())
            .unwrap_or("");
        match class_name {
            "CTFPlayer" => self.handle_player_entity(entity, parser_state),
            "CTFPlayerResource" => self.handle_player_resource(entity, parser_state),
            "CWorld" => self.handle_world_entity(entity, parser_state),
            "CObjectSentrygun" => self.handle_sentry_entity(entity, parser_state),
            "CObjectDispenser" => self.handle_dispenser_entity(entity, parser_state),
            "CObjectTeleporter" => self.handle_teleporter_entity(entity, parser_state),
            _ => {}
        }
    }

    pub fn handle_player_resource(&mut self, entity: &PacketEntity, parser_state: &ParserState) {
        for prop in entity.props(parser_state) {
            if let Some((table_name, prop_name)) = prop.identifier.names() {
                if let Ok(player_id) = u32::from_str(prop_name.as_str()) {
                    let entity_id = EntityId::from(player_id);
                    let mut mappings: Vec<(EntityId, UserId)> = vec![];
                    if let Some(player) = self
                        .state
                        .players
                        .iter_mut()
                        .find(|player| player.entity == entity_id)
                    {
                        match &player.info {
                            Some(info) => {
                                mappings.push((entity_id, info.user_id));
                            }
                            None => {}
                        };
                        match table_name.as_str() {
                            "m_iTeam" => {
                                player.team =
                                    Team::new(i64::try_from(&prop.value).unwrap_or_default())
                            }
                            "m_iMaxHealth" => {
                                player.max_health =
                                    i64::try_from(&prop.value).unwrap_or_default() as u16
                            }
                            "m_iPlayerClass" => {
                                player.class =
                                    Class::new(i64::try_from(&prop.value).unwrap_or_default())
                            }
                            "m_iChargeLevel" => {
                                player.charge = i64::try_from(&prop.value).unwrap_or_default() as u8
                            }
                            "m_iPing" => {
                                player.ping = i64::try_from(&prop.value).unwrap_or_default() as u16
                            }
                            _ => {}
                        }
                    }
                    for (entity_id, user_id) in mappings {
                        self.state.set_entid_to_userid(entity_id, user_id);
                    }
                }
            }
        }
    }

    pub fn handle_player_entity(&mut self, entity: &PacketEntity, parser_state: &ParserState) {
        let player = self.state.get_or_create_player(entity.entity_index);

        const HEALTH_PROP: SendPropIdentifier =
            SendPropIdentifier::new("DT_BasePlayer", "m_iHealth");
        const MAX_HEALTH_PROP: SendPropIdentifier =
            SendPropIdentifier::new("DT_BasePlayer", "m_iMaxHealth");
        const LIFE_STATE_PROP: SendPropIdentifier =
            SendPropIdentifier::new("DT_BasePlayer", "m_lifeState");

        const LOCAL_ORIGIN: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFLocalPlayerExclusive", "m_vecOrigin");
        const NON_LOCAL_ORIGIN: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_vecOrigin");
        const LOCAL_ORIGIN_Z: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFLocalPlayerExclusive", "m_vecOrigin[2]");
        const NON_LOCAL_ORIGIN_Z: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_vecOrigin[2]");
        const LOCAL_EYE_ANGLES: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFLocalPlayerExclusive", "m_angEyeAngles[1]");
        const NON_LOCAL_EYE_ANGLES: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_angEyeAngles[1]");
        const LOCAL_PITCH_ANGLES: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFLocalPlayerExclusive", "m_angEyeAngles[0]");
        const NON_LOCAL_PITCH_ANGLES: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_angEyeAngles[0]");

        const SIMTIME_PROP: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseEntity", "m_flSimulationTime");

        player.in_pvs = entity.in_pvs;

        for prop in entity.props(parser_state) {
            match prop.identifier {
                HEALTH_PROP => {
                    player.health = i64::try_from(&prop.value).unwrap_or_default() as u16
                }
                MAX_HEALTH_PROP => {
                    player.max_health = i64::try_from(&prop.value).unwrap_or_default() as u16
                }
                LIFE_STATE_PROP => {
                    player.state = PlayerState::new(i64::try_from(&prop.value).unwrap_or_default())
                }
                LOCAL_ORIGIN | NON_LOCAL_ORIGIN => {
                    let pos_xy = VectorXY::try_from(&prop.value).unwrap_or_default();
                    player.position.x = pos_xy.x;
                    player.position.y = pos_xy.y;
                }
                LOCAL_ORIGIN_Z | NON_LOCAL_ORIGIN_Z => {
                    player.position.z = f32::try_from(&prop.value).unwrap_or_default()
                }
                LOCAL_EYE_ANGLES | NON_LOCAL_EYE_ANGLES => {
                    player.view_angle = f32::try_from(&prop.value).unwrap_or_default()
                }
                LOCAL_PITCH_ANGLES | NON_LOCAL_PITCH_ANGLES => {
                    player.pitch_angle = f32::try_from(&prop.value).unwrap_or_default()
                }
                SIMTIME_PROP => {
                    player.simtime = i64::try_from(&prop.value).unwrap_or_default() as u16
                }
                _ => {}
            }
        }
    }

    pub fn handle_world_entity(&mut self, entity: &PacketEntity, parser_state: &ParserState) {
        if let (
            Some(SendProp {
                value: SendPropValue::Vector(boundary_min),
                ..
            }),
            Some(SendProp {
                value: SendPropValue::Vector(boundary_max),
                ..
            }),
        ) = (
            entity.get_prop_by_name("DT_WORLD", "m_WorldMins", parser_state),
            entity.get_prop_by_name("DT_WORLD", "m_WorldMaxs", parser_state),
        ) {
            self.state.world = Some(World {
                boundary_min,
                boundary_max,
            })
        }
    }

    pub fn handle_sentry_entity(&mut self, entity: &PacketEntity, parser_state: &ParserState) {
        const ANGLE: SendPropIdentifier =
            SendPropIdentifier::new("DT_TFNonLocalPlayerExclusive", "m_angEyeAngles[1]");
        const MINI: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseObject", "m_bMiniBuilding");
        const CONTROLLED: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectSentrygun", "m_bPlayerControlled");
        const TARGET: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectSentrygun", "m_hAutoAimTarget");
        const SHELLS: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectSentrygun", "m_iAmmoShells");
        const ROCKETS: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectSentrygun", "m_iAmmoRockets");

        if entity.update_type == UpdateType::Delete {
            self.state.remove_building(entity.entity_index);
            return;
        }

        self.handle_building(entity, parser_state, BuildingClass::Sentry);

        let building = self
            .state
            .get_or_create_building(entity.entity_index, BuildingClass::Sentry);

        if let Building::Sentry(sentry) = building {
            for prop in entity.props(parser_state) {
                match prop.identifier {
                    ANGLE => sentry.angle = f32::try_from(&prop.value).unwrap_or_default(),
                    MINI => sentry.is_mini = i64::try_from(&prop.value).unwrap_or_default() > 0,
                    CONTROLLED => {
                        sentry.player_controlled =
                            i64::try_from(&prop.value).unwrap_or_default() > 0
                    }
                    TARGET => {
                        sentry.auto_aim_target =
                            UserId::from(i64::try_from(&prop.value).unwrap_or_default() as u16)
                    }
                    SHELLS => sentry.shells = i64::try_from(&prop.value).unwrap_or_default() as u16,
                    ROCKETS => {
                        sentry.rockets = i64::try_from(&prop.value).unwrap_or_default() as u16
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn handle_teleporter_entity(&mut self, entity: &PacketEntity, parser_state: &ParserState) {
        const RECHARGE_TIME: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectTeleporter", "m_flRechargeTime");
        const RECHARGE_DURATION: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectTeleporter", "m_flCurrentRechargeDuration");
        const TIMES_USED: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectTeleporter", "m_iTimesUsed");
        const OTHER_END: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectTeleporter", "m_bMatchBuilding");
        const YAW_TO_EXIT: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectTeleporter", "m_flYawToExit");
        const IS_ENTRANCE: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseObject", "m_iObjectMode");

        if entity.update_type == UpdateType::Delete {
            self.state.remove_building(entity.entity_index);
            return;
        }

        self.handle_building(entity, parser_state, BuildingClass::Teleporter);

        let building = self
            .state
            .get_or_create_building(entity.entity_index, BuildingClass::Teleporter);

        if let Building::Teleporter(teleporter) = building {
            for prop in entity.props(parser_state) {
                match prop.identifier {
                    RECHARGE_TIME => {
                        teleporter.recharge_time = f32::try_from(&prop.value).unwrap_or_default()
                    }
                    RECHARGE_DURATION => {
                        teleporter.recharge_duration =
                            f32::try_from(&prop.value).unwrap_or_default()
                    }
                    TIMES_USED => {
                        teleporter.times_used =
                            i64::try_from(&prop.value).unwrap_or_default() as u16
                    }
                    OTHER_END => {
                        teleporter.other_end =
                            EntityId::from(i64::try_from(&prop.value).unwrap_or_default() as u32)
                    }
                    YAW_TO_EXIT => {
                        teleporter.yaw_to_exit = f32::try_from(&prop.value).unwrap_or_default()
                    }
                    IS_ENTRANCE => {
                        teleporter.is_entrance = i64::try_from(&prop.value).unwrap_or_default() == 0
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn handle_dispenser_entity(&mut self, entity: &PacketEntity, parser_state: &ParserState) {
        const AMMO: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectDispenser", "m_iAmmoMetal");
        const HEALING: SendPropIdentifier =
            SendPropIdentifier::new("DT_ObjectDispenser", "healing_array");

        if entity.update_type == UpdateType::Delete {
            self.state.remove_building(entity.entity_index);
            return;
        }

        self.handle_building(entity, parser_state, BuildingClass::Dispenser);

        let building = self
            .state
            .get_or_create_building(entity.entity_index, BuildingClass::Dispenser);

        if let Building::Dispenser(dispenser) = building {
            for prop in entity.props(parser_state) {
                match prop.identifier {
                    AMMO => dispenser.metal = i64::try_from(&prop.value).unwrap_or_default() as u16,
                    HEALING => {
                        let values = match &prop.value {
                            SendPropValue::Array(vec) => vec.as_slice(),
                            _ => Default::default(),
                        };

                        dispenser.healing = values
                            .iter()
                            .map(|val| UserId::from(i64::try_from(val).unwrap_or_default() as u16))
                            .collect()
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_building(
        &mut self,
        entity: &PacketEntity,
        parser_state: &ParserState,
        class: BuildingClass,
    ) {
        let building = self
            .state
            .get_or_create_building(entity.entity_index, class);

        const LOCAL_ORIGIN: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseEntity", "m_vecOrigin");
        const TEAM: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_iTeamNum");
        const ANGLE: SendPropIdentifier = SendPropIdentifier::new("DT_BaseEntity", "m_angRotation");
        const SAPPED: SendPropIdentifier = SendPropIdentifier::new("DT_BaseObject", "m_bHasSapper");
        const BUILDING: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseObject", "m_bBuilding");
        const LEVEL: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseObject", "m_iUpgradeLevel");
        const BUILDER: SendPropIdentifier = SendPropIdentifier::new("DT_BaseObject", "m_hBuilder");
        const MAX_HEALTH: SendPropIdentifier =
            SendPropIdentifier::new("DT_BaseObject", "m_iMaxHealth");
        const HEALTH: SendPropIdentifier = SendPropIdentifier::new("DT_BaseObject", "m_iHealth");

        match building {
            Building::Sentry(Sentry {
                position,
                team,
                angle,
                sapped,
                builder,
                level,
                building,
                max_health,
                health,
                ..
            })
            | Building::Dispenser(Dispenser {
                position,
                team,
                angle,
                sapped,
                builder,
                level,
                building,
                max_health,
                health,
                ..
            })
            | Building::Teleporter(Teleporter {
                position,
                team,
                angle,
                sapped,
                builder,
                level,
                building,
                max_health,
                health,
                ..
            }) => {
                for prop in entity.props(parser_state) {
                    match prop.identifier {
                        LOCAL_ORIGIN => {
                            *position = Vector::try_from(&prop.value).unwrap_or_default()
                        }
                        TEAM => *team = Team::new(i64::try_from(&prop.value).unwrap_or_default()),
                        ANGLE => *angle = f32::try_from(&prop.value).unwrap_or_default(),
                        SAPPED => *sapped = i64::try_from(&prop.value).unwrap_or_default() > 0,
                        BUILDING => *building = i64::try_from(&prop.value).unwrap_or_default() > 0,
                        LEVEL => *level = i64::try_from(&prop.value).unwrap_or_default() as u8,
                        BUILDER => {
                            *builder =
                                UserId::from(i64::try_from(&prop.value).unwrap_or_default() as u16)
                        }
                        MAX_HEALTH => {
                            *max_health = i64::try_from(&prop.value).unwrap_or_default() as u16
                        }
                        HEALTH => *health = i64::try_from(&prop.value).unwrap_or_default() as u16,
                        _ => {}
                    }
                }
            }
        }
    }

    fn parse_user_info(
        &mut self,
        index: usize,
        text: Option<&str>,
        data: Option<Stream>,
    ) -> ReadResult<()> {
        if let Some(user_info) =
            tf_demo_parser::demo::data::UserInfo::parse_from_string_table(index as u16, text, data)?
        {
            let ent_id = user_info.entity_id;
            self.state
                .set_entid_to_userid(ent_id, user_info.player_info.user_id.clone());
            match SteamID::from_steam3(&user_info.player_info.steam_id) {
                Ok(steam_id) => self
                    .state
                    .set_userid_to_id64(user_info.player_info.user_id.clone(), steam_id.into()),
                Err(_) => {}
            }
            self.state.get_or_create_player(ent_id).info = Some(user_info.into());
        }

        Ok(())
    }
}
