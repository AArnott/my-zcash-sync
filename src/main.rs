#[macro_use]
extern crate lazy_static;

use zingoconfig::{self, construct_lightwalletd_uri, ChainType, ZingoConfig};
use zingolib::{commands, lightclient::LightClient};

use std::time::Duration;
use std::{cell::RefCell, sync::Arc, sync::Mutex, thread};

// We'll use a MUTEX to store a global lightclient instance,
// so we don't have to keep creating it. We need to store it here, in rust
// because we can't return such a complex structure back to our client.
lazy_static! {
    static ref LIGHTCLIENT: Mutex<RefCell<Option<Arc<LightClient>>>> =
        Mutex::new(RefCell::new(None));
}

fn main() {
    let server_uri = "https://zcash.mysideoftheweb.com:9067/";
    let seed = if wallet_exists(server_uri.to_string()) {
        println!("Initializing existing wallet");
        initialize_existing(server_uri.to_string())
    } else {
        println!("Initializing new wallet");
        initialize_new(server_uri.to_string())
    };

    println!("Initialize: {:?}", seed);
    let sync_result = exec("sync".to_string(), "".to_string());
    println!("sync: {:?}", sync_result);

    // Repeat sync status checks every second until the sync is complete.
    loop {
        thread::sleep(Duration::from_secs(1));
        let sync_status_result = exec("syncstatus".to_string(), "".to_string());
        println!("{:?}", sync_status_result);
    }

    deinitialize();
    ()
}

// Check if there is an existing wallet
fn wallet_exists(server_uri: String) -> bool {
    let server = construct_lightwalletd_uri(Some(server_uri));
    let chaintype = ChainType::Mainnet;
    let config = ZingoConfig::create_unconnected(chaintype, None);

    config.wallet_exists()
}

/// Create a new wallet and return the seed for the newly created wallet.
fn initialize_new(server_uri: String) -> String {
    let server = construct_lightwalletd_uri(Some(server_uri));
    let chaintype = ChainType::Mainnet;
    let block_height = match zingolib::get_latest_block_height(server.clone())
        .map_err(|e| format! {"Error: {e}"})
    {
        Ok(height) => height,
        Err(e) => return e,
    };

    let config = match zingolib::load_clientconfig(server, None, chaintype, true) {
        Ok(c) => c,
        Err(e) => {
            return format!("Error: {}", e);
        }
    };

    let lightclient = match LightClient::new(&config, block_height.saturating_sub(100)) {
        Ok(l) => l,
        Err(e) => {
            return format!("Error: {}", e);
        }
    };

    // Initialize logging
    let _ = LightClient::init_logging();

    let seed = match lightclient.do_seed_phrase_sync() {
        Ok(s) => s.dump(),
        Err(e) => {
            return format!("Error: {}", e);
        }
    };

    let lc = Arc::new(lightclient);
    LightClient::start_mempool_monitor(lc.clone());

    LIGHTCLIENT.lock().unwrap().replace(Some(lc));

    // Return the wallet's seed
    seed
}

// Initialize a new lightclient and store its value
fn initialize_existing(server_uri: String) -> String {
    let server = construct_lightwalletd_uri(Some(server_uri));
    let chaintype = ChainType::Mainnet;

    let config = match zingolib::load_clientconfig(server, None, chaintype, true) {
        Ok(c) => c,
        Err(e) => {
            return format!("Error: {}", e);
        }
    };

    let lightclient = match LightClient::read_wallet_from_disk(&config) {
        Ok(l) => l,
        Err(e) => {
            return format!("Error: {}", e);
        }
    };

    // Initialize logging
    let _ = LightClient::init_logging();

    let lc = Arc::new(lightclient);
    LightClient::start_mempool_monitor(lc.clone());

    LIGHTCLIENT.lock().unwrap().replace(Some(lc));

    "OK".to_string()
}

fn deinitialize() {
    LIGHTCLIENT.lock().unwrap().replace(None);
}

fn exec(cmd: String, args_list: String) -> String {
    let lightclient: Arc<LightClient>;
    {
        let lc = LIGHTCLIENT.lock().unwrap();

        if lc.borrow().is_none() {
            return format!("Error: Light Client is not initialized");
        }

        lightclient = lc.borrow().as_ref().unwrap().clone();
    };

    if cmd == "sync" || cmd == "rescan" || cmd == "import" {
        thread::spawn(move || {
            let args = vec![&args_list[..]];
            commands::do_user_command(&cmd, &args, lightclient.as_ref());
        });

        "OK".to_string()
    } else {
        let args = if args_list.is_empty() {
            vec![]
        } else {
            vec![&args_list[..]]
        };
        commands::do_user_command(&cmd, &args, lightclient.as_ref()).clone()
    }
}
