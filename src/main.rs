extern crate notify;
extern crate msfs;

use std::sync::mpsc::{channel, Sender};
use notify::{watcher, Watcher, RecursiveMode, DebouncedEvent};
use std::time::Duration;
use std::path::PathBuf;
use std::{fs, thread, env};
use msfs::sim_connect::{data_definition, Period, SimConnect, SIMCONNECT_OBJECT_ID_USER, SimConnectRecv};
use std::sync::{RwLock, Arc};
use std::fs::File;
use std::io::{Write, Error};
use std::env::VarError;
use notify::Error::PathNotFound;

struct Position {
    lat: Arc<RwLock<f64>>,
    lon: Arc<RwLock<f64>>,
}

fn get_path() -> Option<PathBuf> {

    let args: Vec<String> = env::args().collect();

    if args.len() == 2 {
        return Some(PathBuf::from(&args[1].to_string()));
    }
    let from_env = env::var("MSFS_SCREENSHOT_FOLDER");
    match from_env {
        Ok(s) => {
            return Some(PathBuf::from(s.to_string()));
        }
        Err(_) => {}
    }

    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let path = get_path();

    if path.is_none() {
        println!("Please provide the path to the folder where your screenshots are stored as an argument or in the Environment as MSFS_SCREENSHOT_FOLDER");
        return Ok(());
    }

    let path = path.unwrap();

    if !path.exists() || !path.is_dir() {
        println!("Please provide a valid path to the folder where your screenshots are stored");
        return Ok(())
    }

    let current_pos = Position{
        lat: Arc::new(RwLock::new(0.0)),
        lon: Arc::new(RwLock::new(0.0))
    };

    let (pos_tx, pos_rx) = channel();

    thread::spawn(move || {
        loop {
            // this function will only return if it couldn't connect.
            fetch_position(pos_tx.clone());
            std::thread::sleep(std::time::Duration::from_secs(5));
        }

    });
    let rec_lat = current_pos.lat.clone();
    let rec_lon = current_pos.lon.clone();
    thread::spawn(move || {
        loop {
            let value = pos_rx.recv().expect("Unable to receive from channel");
            //println!("{:?}", value);
            let lat = rec_lat.write();
            match lat {
                Ok(mut v) => { *v = value.latitude}
                Err(_) => { println!("could not write to current position")}
            }
            let lon = rec_lon.write();
            match lon {
                Ok(mut v) => { *v = value.longitude}
                Err(_) => { println!("could not write to current position")}
            }
        }
    });

    let (tx, rx) = channel();

    let mut watcher = watcher(tx, Duration::from_secs(5))
        .expect("Could not create file watcher");

    watcher
        .watch(path, RecursiveMode::NonRecursive)
        .expect("Could not watch path");

    let rec2_lat = current_pos.lat.clone();
    let rec2_lon = current_pos.lon.clone();

    loop {
        match rx.recv() {
            Ok(event) => {
                println!("{:?}", event);
                match event {
                    DebouncedEvent::Create(e) => {
                        let lat = rec2_lat.read().unwrap();
                        let lon = rec2_lon.read().unwrap();
                        handle_create(&e, *lat, *lon);
                    }
                    DebouncedEvent::Error(e, _) => {
                        println!("Error: {:?}", e);
                    }
                    _ => {
                        println!("Ignoring {:?}", event);
                    }
                }
            }
            Err(e) => {
                println!("Error: {:?}", e);
            }
        }
    }

    Ok(())
}

fn handle_create(path: &PathBuf, lat: f64, lon: f64) {
    let metadata = fs::metadata(path).expect("Could not read metadata of new file");
    let file_type = metadata.file_type();

    if !file_type.is_file() {
        println!("Ignoring non-file {:?}", path);
        return;
    }

    let ext = path.extension().expect("Expected an extension in the filename");
    if ext != "png" && ext != "jpg" && ext != "jpeg" {
        println!("Ignoring non-image {:?}", path);
        return;
    }

    println!("Writing Latitude={}, Longitutde={}", lat, lon);
    let mut target_path = path.clone();
    target_path.set_extension("geo");
    println!("Target path: {:?}", target_path);

    let mut file = File::create(target_path).unwrap();
    file.write_all(format!("{},{}", lat, lon).as_bytes()).expect("Could not write to file");
}

#[data_definition]
#[derive(Debug, Clone, Copy)]
struct Data {
    #[name = "Plane Latitude"]
    #[unit = "degrees"]
    latitude: f64,
    #[name = "Plane Longitude"]
    #[unit = "degrees"]
    longitude: f64,
}

fn fetch_position(tx: Sender<Data>) -> Result<(), String> {
    let sim = SimConnect::open("LOG", |sim, recv| match recv {
        SimConnectRecv::SimObjectData(event) => match event.dwRequestID {
            0 => {
                //println!("{:?}", event.into::<Data>(sim).unwrap());
                let data = event.into::<Data>(sim).unwrap();
                tx.send(data.to_owned());
            }
            _ => {}
        },
        _ => {}
    });

    match sim {
        Ok(mut sim) => {
            println!("Successfully connected to sim!");
            sim.request_data_on_sim_object::<Data>(0, SIMCONNECT_OBJECT_ID_USER, Period::SimFrame).expect("request error");

            loop {
                match sim.call_dispatch() {
                    Ok(_) => {}
                    Err(_) => {
                        return Err("Error connecting to sim.. retrying.".to_string());
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        Err(_) => {
            println!("Error connecting to sim. Is it running? I will retry in a few....")
        }
    }

    Err("Error connecting to sim.. retrying.".to_string())
}


