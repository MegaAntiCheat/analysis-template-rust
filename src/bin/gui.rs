use std::collections::HashMap;

use analysis_template::{
    base::cheat_analyser_base::CheatAnalyser,
    lib::{
        algorithm::{analyse, get_algorithms, Detection},
        parameters::{Parameter, Parameters},
    },
};
use eframe::egui;
use itertools::Itertools;
use tf_demo_parser::Demo;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 800.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "Demo Analysis",
        options,
        Box::new(|_cc| Ok(Box::new(Gui::new()))),
    )
}

struct Gui {
    algos: HashMap<String, bool>,
    params: HashMap<String, Parameters>,
    file: Option<std::path::PathBuf>,
    processing: bool,
    detections: HashMap<u64, Vec<Detection>>,
    selected_player: Option<u64>,
    selected_detection: Option<usize>,

    analyser: Option<CheatAnalyser<'static>>,

    recv: std::sync::mpsc::Receiver<anyhow::Result<CheatAnalyser<'static>>>,
    send: std::sync::mpsc::Sender<anyhow::Result<CheatAnalyser<'static>>>,
}

impl Gui {
    pub fn new() -> Self {
        let mut params: HashMap<String, Parameters> = HashMap::new();
        for mut a in get_algorithms().drain(..) {
            if a.params().is_some() {
                params.insert(a.algorithm_name().to_string(), a.params().cloned().unwrap());
            }
        }
        if let Ok(data) = std::fs::read_to_string("params.json") {
            if let Ok(saved_params) = serde_json::from_str::<HashMap<String, Parameters>>(&data) {
                for saved_algo in saved_params {
                    if let Some(algo) = params.get_mut(&saved_algo.0) {
                        for saved_param in saved_algo.1 {
                            if let Some(param) = algo.get_mut(saved_param.0.as_str()) {
                                *param = saved_param.1;
                            }
                        }
                    }
                }
            }
        }
        let (send, recv) = std::sync::mpsc::channel();
        Self {
            algos: HashMap::from_iter(
                get_algorithms()
                    .iter()
                    .map(|a| (a.algorithm_name().to_string(), a.default())),
            ),
            params,
            file: None,
            processing: false,
            detections: HashMap::new(),
            selected_player: None,
            selected_detection: None,
            analyser: None,
            recv,
            send,
        }
    }

    fn analyse(&mut self) {
        if self.file.is_none() {
            return;
        }
        self.selected_detection = None;
        self.selected_player = None;
        let mut algorithms = get_algorithms();
        algorithms.retain(|a| self.algos[a.algorithm_name()]);

        for a in algorithms.iter_mut() {
            if let Some(p) = self.params.get(a.algorithm_name()) {
                a.params().as_mut().unwrap().clone_from(p);
            }
        }

        let file = self.file.clone().unwrap();
        let send = self.send.clone();

        std::thread::spawn(move || {
            send.send((|| -> anyhow::Result<CheatAnalyser<'static>> {
                let file = std::fs::read(&file)?;
                let demo: Demo = Demo::new(&file);
                Ok(analyse(&demo, algorithms)?)
            })())
            .unwrap();
        });
        self.processing = true;
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !ctx.input(|i| i.raw.hovered_files.is_empty()) {
            egui::Window::new("Hover")
                .movable(false)
                .title_bar(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, &[0.0, 0.0])
                .show(ctx, |ui| {
                    ui.heading("Drop to analyze");
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.processing {
                ui.disable();
                if let Ok(result) = self.recv.try_recv() {
                    if let Err(e) = &result {
                        println!("Error while parsing demo: {e:#?}");
                    }
                    self.analyser = result.ok();
                    self.processing = false;
                    self.detections.clear();
                    for det in self.analyser.as_ref().unwrap().detections.clone() {
                        self.detections.entry(det.player).or_default().push(det);
                    }
                    self.analyser.as_ref().unwrap().print_detection_summary();
                }
            }
            ui.horizontal(|ui|{
                let mut algo_to_configure = None;
                ui.vertical(|ui|{
                    ui.heading("Algorithms");
                    for mut algo in self.algos.iter_mut().sorted_by_key(|a| a.0) {
                        ui.horizontal(|ui|{
                            ui.checkbox(&mut algo.1, algo.0);
                            if *algo.1 && self.params.get(algo.0).is_some_and(|p|!p.is_empty()) {
                                if ui.small_button("⚙").clicked() {
                                    algo_to_configure = Some(algo.0.clone());
                                }
                            }
                        });
                    }
                });
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);
                egui::ScrollArea::vertical().min_scrolled_height(300.0).show(ui, |ui|{
                    ui.vertical(|ui|{
                        ui.horizontal(|ui|{
                            ui.heading("Parameters");
                            if ui.button("Save").clicked(){
                                std::fs::write("params.json", &serde_json::to_vec_pretty(&self.params).unwrap()).unwrap();
                            }
                        });
                        ui.separator();
                        for (name, params) in self.params.iter_mut().sorted_by_key(|a|a.0) {
                            if !self.algos[name]{
                                continue;
                            }
                            ui.add_space(10.0);
                            let h = ui.heading(name);
                            if algo_to_configure.as_ref().is_some_and(|n|*n == *name) {
                                h.scroll_to_me(Some(egui::Align::TOP));
                            }
                            ui.separator();
                            for param in params.iter_mut().sorted_by_key(|p|p.0){
                                match param.1 {
                                    Parameter::Float(f) => {
                                    ui.horizontal(|ui|{
                                        ui.add(egui::DragValue::new(f).speed(0.001).max_decimals(50));
                                        ui.label(param.0);
                                    });
                                    }
                                    Parameter::Int(i) => {
                                        ui.horizontal(|ui|{
                                            ui.add(egui::DragValue::new(i).speed(1).max_decimals(0));
                                            ui.label(param.0);
                                        });
                                    }
                                    Parameter::Bool(b) => {
                                        ui.horizontal(|ui|{
                                            ui.checkbox(b, param.0);
                                        });
                                    }
                                }
                            }
                        }
                    });
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Open...").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Demos", &["dem"])
                        .pick_file()
                    {
                        self.file = Some(path);
                    }
                }
                ui.add_enabled_ui(self.file.is_some(), |ui|{
                    if ui.button("Analyse").clicked() {
                        self.analyse();
                    }
                    if ui.button("Save detections").clicked() {
                        if let Some(a) = &self.analyser {
                            if let Some(path) = rfd::FileDialog::new().set_file_name("detections.json").save_file(){
                                let analysis = serde_json::json!({
                                    "server_ip": a.header.as_ref().map_or("unknown".to_string(), |h| h.server.clone()),
                                    "duration": a.tick,
                                    "author": a.header.as_ref().map_or("unknown".to_string(), |h| h.nick.clone()),
                                    "map": a.header.as_ref().map_or("unknown".to_string(), |h| h.map.clone()),
                                    "detections": a.detections
                                });
                                std::fs::write(path, serde_json::to_vec_pretty(&analysis).unwrap()).unwrap();
                            }
                        }
                    }
                });
            });
            if self.processing {
                ui.horizontal(|ui|{
                    ui.spinner();
                    ui.label("Analysing...");
                    let progress = analysis_template::PROGRESS_CURRENT.load(std::sync::atomic::Ordering::Relaxed);
                    let total = analysis_template::PROGRESS_TOTAL.load(std::sync::atomic::Ordering::Relaxed);
                    ui.add(egui::widgets::ProgressBar::new(progress as f32 / total as f32).show_percentage().text(format!("{} / {}", progress, total)));
                });
            }
            ui.add_space(10.0);
            if let Some(p) = &self.file {
                ui.heading(p.file_name().unwrap().to_string_lossy());
                ui.label("Doubleclick steamid to open profile");
            }
            ui.separator();
            ui.horizontal_top(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("players")
                    .show(ui, |ui| {
                        ui.set_min_width(160.0);
                        ui.vertical(|ui| {
                            for player in self.detections.iter().sorted_by_key(|d| d.1.len()).rev()
                            {
                                let res = ui.selectable_label(
                                    self.selected_player.is_some_and(|u| u == *player.0),
                                    format!("{} ({})", player.0, player.1.len()),
                                );
                                if res.clicked() {
                                    self.selected_player = Some(*player.0);
                                    self.selected_detection = None;
                                }
                                if res.double_clicked() {
                                    let _ = opener::open_browser(format!(
                                        "https://steamcommunity.com/profiles/{}",
                                        player.0
                                    ));
                                }
                            }
                        });
                    });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("detections")
                    .show(ui, |ui| {
                        ui.set_min_width(160.0);
                        ui.vertical(|ui| {
                            if let Some(detections) =
                                self.selected_player.and_then(|p| self.detections.get(&p))
                            {
                                for (i, det) in detections.iter().enumerate() {
                                    if ui
                                        .selectable_label(
                                            self.selected_detection.is_some_and(|si| si == i),
                                            format!("{}: {}", det.tick, det.algorithm),
                                        )
                                        .clicked()
                                    {
                                        self.selected_detection = Some(i);
                                    }
                                }
                            }
                        });
                    });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("details")
                    .show(ui, |ui| {
                        if let Some(det) = self
                            .selected_player
                            .and_then(|p| self.detections.get(&p))
                            .and_then(|dets| self.selected_detection.and_then(|di| dets.get(di)))
                        {
                            ui.label(serde_json::to_string_pretty(&det.data).unwrap());
                        }
                    });
            });
        });
        if let Some(f) = ctx.input(|i| i.raw.dropped_files.first().cloned()) {
            self.file = f.path;
            self.analyse();
        }
    }
}
