use serde_json::Value;

use tf_demo_parser::demo::header::Header;
use tf_demo_parser::demo::parser::{gamestateanalyser::{GameState, GameStateAnalyser}, DemoTicker};
pub use tf_demo_parser::{Demo, DemoParser, Parse, ParseError, ParserState, Stream};

use crate::DemoTickEvent;

pub fn perform_tick<'a> (header: &Header, ticker: &mut DemoTicker<GameStateAnalyser>, mut events: Vec<Box<dyn DemoTickEvent + 'a>>) {
  
    let mut ticker_result: Result<bool, ParseError> = Ok(true);
    let mut last_update = std::time::Instant::now();
    let mut prior_tick: u32 = 1;
    let start = std::time::Instant::now();

    println!("Starting analysis...");

    while ticker_result.is_ok_and(|b| b) { 

        // Get the GameState from the parser

        let state: &GameState = ticker.state();

        if state.tick == prior_tick {
            ticker_result = ticker.tick();
            continue;
        }

        if last_update.elapsed().as_secs() >= 1 {
            println!("Processing tick {} ({} remaining)", state.tick, header.ticks - u32::from(state.tick));
            last_update = std::time::Instant::now();
        }

        let mut json = get_gamestate_json(state);
        json = modify_json(&mut json);

        for event in events.iter_mut() {
            event.on_tick(json.clone()).unwrap();
        }
        
        prior_tick = u32::from(state.tick);

        ticker_result = ticker.tick();
    }

    for event in events.iter_mut() {
        event.finish(); // Fire the end event.
    }

    println!("Done! (Processed {} ticks in {} seconds)", header.ticks, start.elapsed().as_secs());
}

fn get_gamestate_json(state: &GameState) -> Value {
    serde_json::to_value(state).unwrap()
}

fn modify_json(state_json: &mut Value) -> Value {
    let json_object = state_json.as_object_mut().unwrap();

    // Remove kills as it is cumulative (only need latest value)
    // TODO: remove this once the parser is updated to not cumulate kill events
    json_object.remove("kills");

    json_object.entry("players".to_string()).and_modify(|v| {
        let players = v.as_array_mut().unwrap();
        *players = players.iter().filter(|p| p["in_pvs"].as_bool().unwrap()).cloned().collect();
    });

    return serde_json::to_value(json_object).unwrap();
}