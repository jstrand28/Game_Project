use std::{fs, path::PathBuf};

use bevy::app::AppExit;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{game::{MerchantBuyback, MerchantState, MerchantStore, PlayerWallet, Stash}, inventory::{BagGrid, Equipment}};

const SAVE_FILE: &str = "savegame.json";

#[derive(Serialize, Deserialize)]
struct PersistentInventory {
    bag: BagGrid,
    stash: Stash,
    #[serde(default)]
    equipment: Equipment,
    #[serde(default)]
    merchant: MerchantStore,
    #[serde(default)]
    buyback: MerchantBuyback,
    #[serde(default)]
    merchant_state: MerchantState,
    #[serde(default)]
    wallet: PlayerWallet,
}

fn save_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(SAVE_FILE)
}

pub fn load_inventory_persistence(
    mut bag: ResMut<BagGrid>,
    mut stash: ResMut<Stash>,
    mut equipment: ResMut<Equipment>,
    mut merchant: ResMut<MerchantStore>,
    mut buyback: ResMut<MerchantBuyback>,
    mut merchant_state: ResMut<MerchantState>,
    mut wallet: ResMut<PlayerWallet>,
) {
    let Ok(contents) = fs::read_to_string(save_path()) else {
        return;
    };

    let Ok(save) = serde_json::from_str::<PersistentInventory>(&contents) else {
        return;
    };

    *bag = save.bag;
    *stash = save.stash;
    *equipment = save.equipment;
    *merchant = save.merchant;
    *buyback = save.buyback;
    *merchant_state = save.merchant_state;
    *wallet = save.wallet;
}

pub fn save_inventory_on_app_exit(
    mut app_exit_events: MessageReader<AppExit>,
    bag: Res<BagGrid>,
    stash: Res<Stash>,
    equipment: Res<Equipment>,
    merchant: Res<MerchantStore>,
    buyback: Res<MerchantBuyback>,
    merchant_state: Res<MerchantState>,
    wallet: Res<PlayerWallet>,
) {
    if app_exit_events.read().next().is_none() {
        return;
    }

    let save = PersistentInventory {
        bag: bag.clone(),
        stash: stash.clone(),
        equipment: equipment.clone(),
        merchant: merchant.clone(),
        buyback: buyback.clone(),
        merchant_state: merchant_state.clone(),
        wallet: wallet.clone(),
    };

    let Ok(serialized) = serde_json::to_string_pretty(&save) else {
        return;
    };

    let _ = fs::write(save_path(), serialized);
}