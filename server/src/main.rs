//=============================================================================//
// Horizon Game Server - Core Implementation                                   //
//=============================================================================//
// A high-performance, multithreaded game server using Socket.IO for real-time //
// communication. Features include:                                            //
//                                                                             //
// - Scalable thread pool architecture supporting up to 32,000 concurrent      //
//    players                                                                  //
// - Dynamic player connection management with automatic load balancing        //
// - Integrated plugin system for extensible functionality                     //
// - Comprehensive logging and monitoring                                      //
// - Real-time Socket.IO event handling                                        //
// - Graceful error handling and connection management                         //
//                                                                             //
// Structure:                                                                  //
// - Player connections are distributed across multiple thread pools           //
// - Each pool manages up to 1000 players independently                        //
// - Message passing system for inter-thread communication                     //
// - Asynchronous event handling using Tokio runtime                           //
//                                                                             //
// Authors: Tristan James Poland, Thiago M. R. Goulart, Michael Houston,       //
//           Caznix                                                            //
// License: Apache-2.0                                                         //
//=============================================================================//

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use splash::splash;
use std::sync::Once;

use horizon_logger::{log_info, HorizonLogger};

mod server;
mod splash;
// mod collision;

static CTRL_C_HANDLER: Once = Once::new();

//------------------------------------------------------------------------------
// Global Logger Configuration
//------------------------------------------------------------------------------

/// Global logger instance using lazy initialization
/// This ensures the logger is only created when first accessed
pub static LOGGER: Lazy<HorizonLogger> = Lazy::new(|| {
    let logger = HorizonLogger::new();
    logger
});

#[tokio::main]
async fn main() -> Result<()> {
    //collision::main();

    splash();
    let config_init_time = std::time::Instant::now();
    log_info!(
        LOGGER,
        "INIT",
        "Server config loaded in {:#?}",
        config_init_time.elapsed()
    );

    let init_time = std::time::Instant::now();

    // Start the server
    server::start().await.context("Failed to start server")?;
    println!("Server started in {:#?}", init_time.elapsed());

    let mut terminating: bool = false;

    CTRL_C_HANDLER.call_once(|| {
        // Register the Ctrl+C handler
        ctrlc::set_handler(move || {
            if !terminating {
                terminating = true;

                println!("Exit");
                std::process::exit(0);
            }
        })
        .expect("Failed to register ctrl+c handler");
    });
    Ok(())
}