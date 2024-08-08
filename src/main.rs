/////////////////////////////////////////////////////////////////////////////////////////////////////
//                                       Horizon Game Server                                       //
//                                                                                                 //
//  This server software is part of a distributed system designed to facilitate communication      //
//  and data transfer between multiple child servers and a master server. Each child server        //
//  operates within a "Region map" managed by the master server, which keeps track of their        //
//  coordinates in a relative cubic light-year space. The coordinates are stored in 64-bit floats  //
//  to avoid coordinate overflow and to ensure high precision.                                     //
//                                                                                                 //
/////////////////////////////////////////////////////////////////////////////////////////////////////

////////////////////////////////////////////////////////////////////
// Use the mimalloc allocator, which boasts excellent performance //
// across a variety of tasks, while being small (8k LOC)          //
////////////////////////////////////////////////////////////////////
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

///////////////////////////////////////////
// Import a few things to get us started //
///////////////////////////////////////////

// Imported some third party crates
use http::status;
use serde_json::{json, Value};
use socketioxide::extract::{AckSender, Bin, Data, SocketRef};
use std::sync::{Arc, Mutex};
use tokio::{main, task::spawn};
use tracing::{debug, info};
use viz::{future::ok, handler::ServiceHandler, serve, Response, Result, Router, Request, Body};


// Import some custom crates from the crates folder in /src
use TerraForge;
use PebbleVault;

//////////////////////////////////////////////////////////////
//                    !!!! WARNING !!!!                     //
// Import all structs (when we have a ton of structs this   //
// will be very bad but should be fine for now)             //
//////////////////////////////////////////////////////////////
use structs::*;

/////////////////////////////////////
// Import the modules we will need //
/////////////////////////////////////

mod events;
mod macros;
mod structs;
mod subsystems;

///////////////////////////////////////////////////////////////
//                    !!!! WARNING !!!!                      //
// on_connect runs every time a new player connects to the   //
// server avoid putting memory hungry code here if possible! //
///////////////////////////////////////////////////////////////

fn on_connect(socket: SocketRef, Data(data): Data<Value>, players: Arc<Mutex<Vec<Player>>>) {
    // Update the parsing functions to handle the data without Object wrappers

    fn parse_rotation(parse: &Value) -> (f64, f64, f64, f64) {
        (
            parse_f64(&parse["w"]).unwrap_or(0.0),
            parse_f64(&parse["x"]).unwrap_or(0.0),
            parse_f64(&parse["y"]).unwrap_or(0.0),
            parse_f64(&parse["z"]).unwrap_or(0.0),
        )
    }

    fn parse_xyz(parse: &Value) -> (f64, f64, f64) {
        (
            parse_f64(&parse["x"]).unwrap_or(0.0),
            parse_f64(&parse["y"]).unwrap_or(0.0),
            parse_f64(&parse["z"]).unwrap_or(0.0),
        )
    }

    fn parse_xy(parse: &Value) -> (f64, f64) {
        (
            parse_f64(&parse["x"]).unwrap_or(0.0),
            parse_f64(&parse["y"]).unwrap_or(0.0),
        )
    }

    fn parse_f64(n: &Value) -> Result<f64, std::io::Error> {
        n.as_f64().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid f64 value"))
    }


    let id = socket.id.as_str();
    println!("Starting subsystems for player: {}", id);

    ///////////////////////////////////////////////////////////////////
    //
    //
    ///////////////////////////////////////////////////////////////////

    // Authenticate the user
    let player =  Player {
        socket: socket.clone(),
        moveActionValue: None,
        transform: None
    };

    players.lock().unwrap().push(player);
    println!("Player {} added to players list", id);

    println!("Socket.IO connected: {:?} {:?}", socket.ns(), socket.id);
    socket.emit("connected", true).ok();
    socket.emit("auth", true).ok();
         
    ///////////////////////////////////////////////////////////
    //  Setup external event listeners for the more complex  //
    //  systems                                              //
    ///////////////////////////////////////////////////////////
    

    // subsystems::actors::main::main();
    subsystems::core::chat::init(socket.clone());
    // subsystems::core::leaderboard::init();
    // subsystems::player_data::init(socket.clone());
    subsystems::core::game_logic::init();
    subsystems::core::level_data::init();
    subsystems::core::logging::init();
    subsystems::core::notifications::init();
    
    /////////////////////////////////////////////////////////
    //  Register some additional custom events with our    // 
    //  socket server. Your custom events will be          //
    //  registered here as well as in the ./events/mod.rs  //
    //  file                                               //
    /////////////////////////////////////////////////////////
    
    //  define_event!(socket, 
    //      "test", events::test::main(),
    //      );

    let players_clone_updateloc: Arc<Mutex<Vec<Player>>> = Arc::clone(&players);
    let players_clone: Arc<Mutex<Vec<Player>>> = Arc::clone(&players);
    socket.on(
        "updatePlayerLocation",
        move |socket: SocketRef, Data::<Value>(data), Bin(bin)| {
            println!(
                "Received event: UpdatePlayerLocation with data: {:?} and bin: {:?}",
                data, bin
            );
        
            // Extract location from data
            if let Some(transform) = data.get("transform").and_then(|t| t.as_object()) {
                let mut players: std::sync::MutexGuard<Vec<Player>> = players_clone_updateloc.lock().unwrap();
                if let Some(player) = players.iter_mut().find(|p: &&mut Player| p.socket.id == socket.id)
                {
                    // Do the actual parsing
                    if let (Some(rotation), Some(translation), Some(scale3d)) = (
                        transform.get("rotation"),
                        transform.get("translation"),
                        transform.get("scale3D")
                    ) {
                        let (rot_w, rot_x, rot_y, rot_z) = parse_rotation(rotation);
                        let (trans_x, trans_y, trans_z) = parse_xyz(translation);
                        let (scale3d_x, scale3d_y, scale3d_z) = parse_xyz(scale3d);
                    
                        // Create or update the transform
                        let mut transform = player.transform.take().unwrap_or_else(|| structs::Transform {
                            rotation: None,
                            translation: None,
                            scale3D: structs::Scale3D { x: 1.0, y: 1.0, z: 1.0 },
                            location: None,
                        });
                    
                        // Update rotation
                        transform.rotation = Some(structs::Rotation { w: rot_w, x: rot_x, y: rot_y, z: rot_z });
                    
                        // Update translation
                        let new_translation = structs::Translation { x: trans_x, y: trans_y, z: trans_z };
                        transform.translation = Some(new_translation.clone());
                        transform.location = Some(new_translation);
                    
                        // Update scale3D
                        transform.scale3D = structs::Scale3D { x: scale3d_x, y: scale3d_y, z: scale3d_z };
                    
                        // Update the player's transform
                        player.transform = Some(transform);
                    
                        // Parse player movement axis values
                        if let Some(move_action) = data.get("move Action Value") {
                            let (mv_action_value_x, mv_action_value_y) = parse_xy(move_action);
                            player.moveActionValue = Some(structs::MoveActionValue { x: mv_action_value_x, y: mv_action_value_y });
                        }
                    
                        // Print a debug statement
                        println!("Updated player location: {:?}", player);
                    } else {
                        println!("Invalid transform data structure");
                    }
                } else {
                    println!("Player not found: {}", socket.id);
                }
            } else {
                println!("Failed to parse location: transform field not found or is not an object");
            }
        
            // Send a reply containing the correct data
            socket.bin(bin).emit("messageBack", data).ok();
        },
    );

    /////////////////////////////////////////////////////////////////////////
    //  Client sends this message when they need a list of online players  //
    /////////////////////////////////////////////////////////////////////////

    socket.on(
        "getOnlinePlayers",
        move |socket: SocketRef, _: Data<Value>, _: Bin| {
            info!("Responding with online players list");
            let players: std::sync::MutexGuard<Vec<Player>> = players_clone.lock().unwrap();
            let online_players_json = serde_json::to_value(
                players
                    .iter()
                    .map(|player| json!({ "id": player.socket.id }))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            debug!("Player Array as JSON: {}", online_players_json);
            socket.emit("onlinePlayers", online_players_json).ok();
        },
    );

    let players_clone: Arc<Mutex<Vec<Player>>> = Arc::clone(&players);

    socket.on(
        "getPlayersWithLocations",
        move |socket: SocketRef, Data::<Value>(data), ack: AckSender, Bin(bin)| {
            info!("Responding with players and locations list");
            let players: std::sync::MutexGuard<Vec<Player>> = players_clone.lock().unwrap();
            
            match data {
                Value::Null => println!("Received event with null data"),
                Value::String(s) => println!("Received event with string data: {}", s),
                _ => println!("Received event with data: {:?}", data),
            }
            println!("Binary payload: {:?}", bin);
            let players_with_locations_json = serde_json::to_value(
                players
                    .iter()
                    .map(|player| json!({ 
                        "id": player.socket.id, 
                        "transform": player.transform.as_ref().unwrap().location
                    }))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            info!(
                "Players with Locations as JSON: {}",
                players_with_locations_json
            );
            let players = vec![players_with_locations_json];
            socket.emit("playersWithLocations", &players).ok();
        },
    );

    let players_clone: Arc<Mutex<Vec<Player>>> = Arc::clone(&players);
    socket.on("broadcastMessage", move |Data::<Value>(data), _: Bin| {
        let players: std::sync::MutexGuard<Vec<Player>> = players_clone.lock().unwrap();
        for player in &*players {
            player.socket.emit("broadcastMessage", data.clone()).ok();
        }
    });
}


// This handels redirecting browser users to the master server to see the dashboard
async fn redirect_to_master_panel(_req: Request) -> Result<Response> {
    let response = Response::builder()
        .status(302)
        .header("Location", "https://google.com")
        .body(Body::empty())
        .unwrap();
    println!("Someone tried to access this server via a browser, redirecting them to the master dashboard");
    Ok(response)
}

#[main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    /////////////////////////////
    // SERVER STARTUP SEQUENCE //
    /////////////////////////////

    // Show branding
    subsystems::core::startup::main();

    // this is in it's own thread so it does not take up the main thread because this task runs
    // throughout the lifetime of the server and would prevent anything else from running
    let _terraforge_thread = spawn(async {
        // this is in it's own thread to not take up the main thread. because otherwise that would
        // be catastrophically bad for performance, because then the tasks would not complete.
        TerraForge::main();
    });
    
    PebbleVault::main();

    ////////////////////////////////////////////////////////////////////////
    //                              DEBUG ONLY                            //
    // The code below allows for the creation of some test bodies within  //
    // pebblevault, this is normally done automatically by TerraForge.    //
    ////////////////////////////////////////////////////////////////////////

    // Define a place to put new players
    let players: Arc<Mutex<Vec<Player>>> = Arc::new(Mutex::new(Vec::new()));
    let (svc, io) = socketioxide::SocketIo::new_svc();
    let players_clone: Arc<Mutex<Vec<Player>>> = players.clone();

    // Handle New player connections
    io.ns("/", move |socket: SocketRef, data: Data<Value>| {
        println!("Player Connected!");
        on_connect(socket, data, players_clone.clone())
    });
    
    //Create a router to handel incoming connections
    let app = Router::new()
        // If the user sends a GET request we redirect them to
        // the master server which hosts the horizon dashboard
        // if the master server itself has a master it too will
        // redirect them until they reach the highest level master
        // server

        .get("/", redirect_to_master_panel)

        // This is an any connection that is not handled above,
        // we cosider these legitimate players and treat their
        // request as them attempting to join the server
        .any("/*", ServiceHandler::new(svc));


    info!("Starting server");
    
    // Define a listener on port 3000
    let listener: tokio::net::TcpListener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    
    // Print any errors encountered while creating the listener
    if let Err(e) = serve(listener, app).await {
        println!("{}", e);
    }
    Ok(())
}
