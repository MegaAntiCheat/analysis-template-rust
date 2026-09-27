# Demo Analysis Template (Rust)

This repo contains a Rust template for TF2 demo analysis, as well as some very basic implementations.

This repo produces an executable that accepts a demo file as input, and returns a json string containing metadata related to the demo. All algorithms for MegaAntiCheat will be implemented in a private fork; this repo merely acts as a public-facing template.

## How to use this template.

### Getting started

If you have a demo file you want to scan with the included algorithms, you have two options:

#### CLI (default)

After setting up your Rust developer environment, check the program executes correctly with the command `cargo run -- -i "path/to/demo.dem"`. You can also add the `--release` flag after `cargo run` to dramatically speed up the program (2-10x), however this enables some safeguards that prevents panics from certain invalid states (e.g. u32 underflows), so be careful!

Example output: 
```Map: pl_borneo
Duration: 00:11:28.038 (45892 ticks)
User: megascatterbomb (part 2 soon)
Server: 117.120.11.35:6996
Starting analysis...
Processing tick 3238 (42654 remaining, 3237 tps)
Processing tick 5838 (40054 remaining, 2918 tps)
Processing tick 8750 (37142 remaining, 2915 tps)
Processing tick 12013 (33879 remaining, 3002 tps)
Processing tick 14829 (31063 remaining, 2964 tps)
Processing tick 16994 (28898 remaining, 2831 tps)
Processing tick 19277 (26615 remaining, 2752 tps)
Processing tick 21628 (24264 remaining, 2702 tps)
Processing tick 24571 (21321 remaining, 2729 tps)
Processing tick 27254 (18638 remaining, 2724 tps)
Processing tick 29753 (16139 remaining, 2703 tps)
Processing tick 33029 (12863 remaining, 2751 tps)
Processing tick 36511 (9381 remaining, 2807 tps)
Processing tick 39226 (6666 remaining, 2800 tps)
Processing tick 43498 (2394 remaining, 2898 tps)
Map: pl_borneo
Duration: 00:11:28.038 (45892 ticks)
User: megascatterbomb (part 2 soon)
Server: 117.120.11.35:6996
Done! (Processed 45892 ticks in 15.64 seconds averaging 2934.88 tps)
[]
```

The empty array at the bottom means there were no detections. If there were detections, they would look something like this:

```
[
  {
    "tick": 8466,
    "algorithm": "viewangles_180degrees",
    "player": 76561199776113179,
    "data": {
      "pa_delta": -180.0,
      "va_delta": -58.768341064453125
    }
  },
  {
    "tick": 8466,
    "algorithm": "viewangles_180degrees",
    "player": 76561199775364340,
    "data": {
      "pa_delta": -180.0,
      "va_delta": 33.07917404174805
    }
  },
  {
    "tick": 8469,
    "algorithm": "viewangles_180degrees",
    "player": 76561199774314308,
    "data": {
      "pa_delta": -180.0,
      "va_delta": 33.079193115234375
    }
  },
  {
    "tick": 8469,
    "algorithm": "viewangles_180degrees",
    "player": 76561199776113179,
    "data": {
      "pa_delta": 180.0,
      "va_delta": -134.0762424468994
    }
  }
]
```
These are real detections against real cheaters, but the Steam IDs have been substituted for now-deleted bot accounts.

In production, the `-q` flag is used to silence all debug info, leaving only the detection output in stdout. 

#### GUI

The command `cargo build --release --bin gui --features gui` builds a GUI-based program at `target/release` which lets you choose which algorithms you want to use, configure their parameters, and easily access the Steam profiles of detected players.

Note that there's no progress bar at the moment, so if you want to monitor the application as it runs, you need to use the terminal to monitor stdout.

### Output

The output is a json array containing serialized Detection objects. The `Detection` struct is used to represent a single detection event. It contains the following fields:

- `tick: u64`: The tick number at which the detection occurred.
- `algorithm: String`: The name of the algorithm which produced the detection.
- `player: u64`: The Steam ID of the player who triggered the detection.
- `data: Value`: A JSON value containing any relevant data for the detection, such as what viewangles triggered the detection.

### Arguments

The program accepts the following arguments:

- `-a <algorithm> [-a <algorithm>]...`: Specify the algorithms to run. If not specified, the default algorithms are run.
- `-c`: Print the number of detections instead of details for every detection. Overridden by `-q`.
- `-h`: Print help information and exit.
- `-i <path>`: Specify the path to the demo file to analyze. **This argument is required.**
- `-o <path>`: Write the raw JSON output to a file. The file contains the exact same output as `-Q` (pretty-printed), unless `-q` is also specified, in which case it is minified in the same way as `-q`.
- `-p`: Provide a .json file with custom parameters.
- `-q`: Silence all debug info, leaving only the detection output in stdout. Required for production use.
- `-Q`: Same as `-q`, but prettifies the output. Convenient for manual inspection of the output.

### Writing your own algorithm

This section describes the structure of a cheat detection algorithm. You can also view a complete algorithm with supporting comments at `src/algorithms/viewangles_180degrees.rs`.

To write your own algorithm, you must implement the `CheatAlgorithm` trait. To do this, create a new file in the `src/algorithms/` directory. For example, if you want to detect 180 degree snaps, you might create `src/algorithms/viewangles_180degrees.rs`. In this file, you can define whatever structs, types etc you need to create your algorithm. At minimum, you need to implement some of the functions in `CheatAlgorithm`:

- `default(&self) -> bool` (REQUIRED): Should this algorithm run by default if -a isn't specified?
- `algorithm_name(&self) -> &str` (REQUIRED): Return your algorithm's name here. Best practice is to match the filename.
- `params(&mut self) -> Option<&mut Parameters>`: Return any parameters your algorithm takes here (See `src/algorithms/write_to_file.rs` for example usage).
- `handled_messages(&self) -> Vec<MessageType>`: Return any message types that your algorithm needs to parse here. You can access the message objects by implementing `on_message`.
- `init(&mut self) -> Result<(), Error>`: Called before any other events. Use this instead of your object's constructor when performing any non-ephemeral actions e.g. modifying files.
- `on_tick(&mut self, tick: Value) -> Result<Vec<Detection>, Error>`: Called for each tick. A tick is triggered by the receipt of a NetTick message. The game state for the tick is passed in as a CheatAnalyserState.
- `on_message(&mut self, _message: &Message, _state: &CheatAnalyserState, _parser_state: &ParserState, _tick: DemoTick) -> Result<Vec<Detection>, Error>`: Called for each message (i.e. packet) in the demo. Which messages get parsed depends on what message types are being listened for (see `handled_messages`). NetTick messages trigger on_tick and on_message in that order.
- `finish(&mut self) -> Result<Vec<Detection>, Error>`: Called after all other events. Use for cleaning up or for aggregate analysis.

The functions that can return `Ok(Vec<Detection>)` are the main entry points for your algorithm. Your task is to process the incoming data and produce Detection objects for each event where cheating is suspected.

Basic information about the game state is provided as a CheatAnalyserState object via `CheatAlgorithm::on_tick` and `CheatAlgorithm::on_message`. To get more specific information out of the demo, use `handled_messages` and `on_message` to listen for specific message types. To understand the structure of CheatAnalyserState, try `cargo run --release -i "path/to/demo.dem" -a write_to_file` to write all the CheatAnalyserState objects to one large file as json. Each tick is written to a new line.

To register a detection, include it in the vector that's returned at the end of any detection function. Detections don't have to be returned in the same function call that the relevant data is introduced; you can store Detections elsewhere and return them all in CheatAlgorithm::finish() if you want, but make sure all the Detection objects you want to return are returned before the program terminates. This is a good pattern for aggregate detection methods e.g. crit hack detection, or for any situation where you need future information before confirming the detection.

If you don't have any detections to return, just return an empty vector.