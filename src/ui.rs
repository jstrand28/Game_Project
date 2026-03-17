use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_rapier3d::prelude::*;
use rand::{rngs::StdRng, RngExt, SeedableRng};
use std::collections::HashMap;
use crate::{
    game::{ AppState, UIFlags, OpenChest, ActiveSpell, StartMenuState, StartMenuTab, TraderShelfFilter, Spell, Spellbook, Stash, MerchantBuyback, MerchantState, MerchantStore, PlayerWallet, CameraModeSettings, LookAngles, Player, PlayerMotion, Health, PlayerCamera, EnemyHealthBarAnchor, WeaponSlot, weapon_preview_stats, DamagePopup },
    stats::{PlayerStats, Stats},
    inventory::{BagGrid, Equipment, Chest, ChestTier},
    items::{roll_item, Item},
};

#[derive(Resource, Default)]
pub struct UISelection {
    pub selected_bag_cell: Option<(u8,u8)>,
    pub selected_chest_idx: Option<usize>,
}

pub fn start_menu_ui(
    mut ctxs: EguiContexts,
    mut next: ResMut<NextState<AppState>>,
    mut ps: ResMut<PlayerStats>,
    mut stash: ResMut<Stash>,      // stash resource (100 slots)
    mut merchant: ResMut<MerchantStore>,
    mut buyback: ResMut<MerchantBuyback>,
    mut merchant_state: ResMut<MerchantState>,
    mut wallet: ResMut<PlayerWallet>,
    mut bag: ResMut<BagGrid>,      // player starting bag
    mut equip: ResMut<Equipment>,
    mut tabs: ResMut<StartMenuState>, // which tab is active
) {
    let Ok(ctx) = ctxs.ctx_mut() else { return; };

    // Top-level Start screen window
    egui::Window::new("Start Screen")
        .id(egui::Id::new("window.start_screen"))
        .default_size(egui::vec2(700.0, 520.0))
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {

            // --- Tabs header -------------------------------------------------
            ui.horizontal_wrapped(|ui| {
                let main_selected  = tabs.active == StartMenuTab::Main;
                let stash_selected = tabs.active == StartMenuTab::Stash;
                let trader_selected = tabs.active == StartMenuTab::Trader;
                let character_selected = tabs.active == StartMenuTab::Character;

                if ui.selectable_label(main_selected,  "Main Menu").clicked() {
                    tabs.active = StartMenuTab::Main;
                }
                if ui.selectable_label(stash_selected, "Stash").clicked() {
                    tabs.active = StartMenuTab::Stash;
                }
                if ui.selectable_label(trader_selected, "Trader").clicked() {
                    tabs.active = StartMenuTab::Trader;
                }
                if ui.selectable_label(character_selected, "Character").clicked() {
                    tabs.active = StartMenuTab::Character;
                }
            });

            ui.separator();

            // --- Tab content -------------------------------------------------
            let content_height = (ui.available_height() - 6.0).max(220.0);
            egui::ScrollArea::vertical()
                .id_salt(ui.id().with("start_screen_tab_content"))
                .max_height(content_height)
                .auto_shrink([false, false])
                .show(ui, |ui| match tabs.active {
                StartMenuTab::Main => {
                    // === ORIGINAL MAIN MENU CONTENT ===
                    ui.heading("Customize Character");
                    ui.separator();
                    ui.label("Distribute +5 free points (not persisted across sessions yet).");

                    let mut b = ps.base;
                    for (label, val) in [
                        ("Vigor",     &mut b.vigor),
                        ("Strength",  &mut b.strength),
                        ("Agility",   &mut b.agility),
                        ("Magic",     &mut b.magic),
                        ("Endurance", &mut b.endurance),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            if ui.button("-").clicked() { *val = (*val - 1).max(1); }
                            ui.label(format!("{val}"));
                            if ui.button("+").clicked() { *val += 1; }
                        });
                    }

                    ui.separator();
                    if ui.button("Play").clicked() {
                        ps.base = b;
                        ps.recompute_limbs();
                        next.set(AppState::InGame);
                    }

                    // If you also want an Exit button on the start screen,
                    // pass EventWriter<AppExit> into this function and send it here.
                }

                StartMenuTab::Stash => {
                    ui.heading("Inventory Management");
                    ui.label("Move items between Bag, Stash, and Character slots. Equip from Bag or Stash, and click a character slot to unequip it.");
                    ui.separator();

                    let mut pending_bag_move: Option<usize> = None;
                    let mut pending_bag_equip: Option<usize> = None;
                    let mut pending_stash_move: Option<usize> = None;
                    let mut pending_stash_equip: Option<usize> = None;
                    let mut pending_unequip: Option<crate::items::EquipSlot> = None;

                    ui.columns(3, |cols| {
                        cols[0].group(|ui| {
                            ui.set_min_size(egui::vec2(220.0, 380.0));
                            ui.heading(format!("Bag ({}x{})", bag.w, bag.h));
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_salt(ui.id().with("scroll_bag"))
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    let filled = bag.cells.iter().filter(|cell| cell.is_some()).count();
                                    ui.small(format!("{} items", filled));
                                    ui.separator();
                                    for (index, cell) in bag.cells.iter().enumerate() {
                                        if let Some(item) = cell.as_ref() {
                                            let move_clicked = paint_item_action_card(ui, item, "Send to Stash", item.equip_slot.is_some().then_some("Equip"));
                                            if move_clicked.0 {
                                                pending_bag_move = Some(index);
                                            }
                                            if move_clicked.1 {
                                                pending_bag_equip = Some(index);
                                            }
                                        }
                                    }
                                    if filled == 0 {
                                        ui.label("Bag is empty.");
                                    }
                                });
                        });

                        cols[1].group(|ui| {
                            ui.set_min_size(egui::vec2(220.0, 380.0));
                            ui.heading("Stash (100 slots)");
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_salt(ui.id().with("scroll_stash"))
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    let filled = stash.slots.iter().filter(|slot| slot.is_some()).count();
                                    ui.small(format!("{} stored", filled));
                                    ui.separator();
                                    for (index, slot) in stash.slots.iter().enumerate() {
                                        if let Some(item) = slot.as_ref() {
                                            let move_clicked = paint_item_action_card(ui, item, "Move to Bag", item.equip_slot.is_some().then_some("Equip"));
                                            if move_clicked.0 {
                                                pending_stash_move = Some(index);
                                            }
                                            if move_clicked.1 {
                                                pending_stash_equip = Some(index);
                                            }
                                        }
                                    }
                                    if filled == 0 {
                                        ui.label("Stash is empty.");
                                    }
                                });
                        });

                        cols[2].group(|ui| {
                            ui.set_min_size(egui::vec2(220.0, 380.0));
                            ui.heading("Character Slots");
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_salt(ui.id().with("scroll_character_slots"))
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    for (label, slot) in menu_equipment_slots() {
                                        let equipped = equipped_item_for_slot(&equip, slot);
                                        ui.group(|ui| {
                                            ui.set_min_width(180.0);
                                            ui.strong(label);
                                            match equipped {
                                                Some(item) => {
                                                    ui.horizontal(|ui| {
                                                        paint_item_badge(ui, item, egui::vec2(24.0, 24.0));
                                                        ui.vertical(|ui| {
                                                            ui.colored_label(rarity_color32(item), &item.name);
                                                            ui.small(item_descriptor(item));
                                                        });
                                                    });
                                                    if ui.button("Unequip").clicked() {
                                                        pending_unequip = Some(slot);
                                                    }
                                                }
                                                None => {
                                                    ui.label("Empty");
                                                }
                                            }
                                        });
                                        ui.add_space(4.0);
                                    }
                                });
                        });
                    });

                    if let Some(index) = pending_bag_move {
                        if let Some(item) = bag.cells.get_mut(index).and_then(Option::take) {
                            if let Some(slot) = stash.slots.iter_mut().find(|slot| slot.is_none()) {
                                *slot = Some(item);
                            } else {
                                bag.cells[index] = Some(item);
                            }
                        }
                    }

                    if let Some(index) = pending_stash_move {
                        if let Some(item) = stash.slots.get_mut(index).and_then(Option::take) {
                            if !bag.try_add(item.clone()) {
                                stash.slots[index] = Some(item);
                            }
                        }
                    }

                    if let Some(index) = pending_bag_equip {
                        equip_menu_item_from_bag(index, &mut bag, &mut stash, &mut equip, &mut ps);
                    }

                    if let Some(index) = pending_stash_equip {
                        equip_menu_item_from_stash(index, &mut bag, &mut stash, &mut equip, &mut ps);
                    }

                    if let Some(slot) = pending_unequip {
                        unequip_menu_slot(slot, &mut bag, &mut stash, &mut equip, &mut ps);
                    }
                }
                StartMenuTab::Trader => {
                    ui.heading("Merchant");
                    ui.label("Sell any item from your stash to the merchant. Buy items into your bag. Prices move with store stock.");
                    ui.separator();
                    ui.horizontal(|ui| {
                        paint_coin_icon(ui, 18.0);
                        ui.heading(format!("{} gold", wallet.gold.max(0)));
                    });

                    let listed_items = merchant.slots.iter().flatten().count();
                    let avg_price = if listed_items > 0 {
                        let total_price: i32 = merchant
                            .slots
                            .iter()
                            .flatten()
                            .map(|item| merchant_buy_price(item, 1))
                            .sum();
                        total_price / listed_items as i32
                    } else {
                        0
                    };

                    ui.group(|ui| {
                        ui.label(format!("Merchant items listed: {}", listed_items));
                        ui.label(format!("Average shelf price: {}g", avg_price.max(0)));
                        ui.label(format!("Refreshes remaining: {}", merchant_state.refreshes_remaining));
                    });

                    let merchant_counts = merchant_stock_counts(&merchant, &buyback);
                    let mut pending_sell: Option<usize> = None;
                    let mut pending_buy: Option<usize> = None;
                    let mut pending_buyback: Option<usize> = None;
                    let refresh_cost = merchant_refresh_cost(&merchant_state);

                    ui.horizontal(|ui| {
                        let can_refresh = merchant_state.refreshes_remaining > 0 && wallet.gold >= refresh_cost;
                        if ui.add_enabled(can_refresh, egui::Button::new(format!("Refresh stock ({refresh_cost}g)"))).clicked() {
                            wallet.gold -= refresh_cost;
                            merchant_state.refreshes_remaining = merchant_state.refreshes_remaining.saturating_sub(1);
                            merchant_state.refresh_seed = merchant_state.refresh_seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
                            merchant_state.refresh_seed = merchant_state.refresh_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                            refresh_merchant_inventory(&mut merchant, merchant_state.refresh_seed, &merchant_state);
                        }
                        if !can_refresh {
                            ui.label("Need gold and refreshes left.");
                        }
                    });

                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(22, 24, 28))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20)))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Browse stock")
                                    .strong()
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(224, 229, 236)),
                            );
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                                for (label, filter) in [
                                    ("All Items", TraderShelfFilter::All),
                                    ("Weapons", TraderShelfFilter::Weapons),
                                    ("Gear", TraderShelfFilter::Gear),
                                    ("Consumables", TraderShelfFilter::Consumables),
                                    ("Premium", TraderShelfFilter::Premium),
                                ] {
                                    if paint_trader_filter_chip(ui, label, tabs.trader_filter == filter) {
                                        tabs.trader_filter = filter;
                                    }
                                }
                            });
                        });

                    ui.columns(2, |cols| {
                        cols[0].group(|ui| {
                            ui.set_min_size(egui::vec2(332.0, 390.0));
                            ui.heading("Sell From Stash");
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_salt(ui.id().with("scroll_trader_stash"))
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    let filled = stash.slots.iter().filter(|slot| slot.is_some()).count();
                                    ui.small(format!("{} items available", filled));
                                    ui.separator();
                                    for (index, slot) in stash.slots.iter().enumerate() {
                                        if let Some(item) = slot.as_ref() {
                                            let quantity = merchant_item_quantity(item, &merchant_counts);
                                            let price = merchant_sell_price(item, quantity);
                                            if paint_item_action_card(ui, item, &format!("Sell +{}g", price), None).0 {
                                                pending_sell = Some(index);
                                            }
                                        }
                                    }
                                    if filled == 0 {
                                        ui.label("Stash is empty.");
                                    }
                                });
                        });

                        cols[1].group(|ui| {
                            ui.set_min_size(egui::vec2(344.0, 390.0));
                            ui.heading("Merchant Stock");
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_salt(ui.id().with("scroll_trader_store"))
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    let mut visible_stock = 0usize;
                                    for (index, slot) in merchant.slots.iter().enumerate() {
                                        if let Some(item) = slot.as_ref() {
                                            if !matches_trader_filter(item, tabs.trader_filter) {
                                                continue;
                                            }
                                            visible_stock += 1;
                                            let quantity = merchant_item_quantity(item, &merchant_counts);
                                            let price = merchant_buy_price(item, quantity);
                                            let afford = wallet.gold >= price;
                                            if paint_item_buy_card(ui, item, &format!("Buy {}g", price), &format!("Stock {}", quantity), afford) {
                                                pending_buy = Some(index);
                                            }
                                            draw_trader_item_compare(ui, item, &equip, &ps);
                                            ui.add_space(4.0);
                                        }
                                    }
                                    if visible_stock == 0 {
                                        ui.label("No merchant items match this filter.");
                                    }
                                });

                            ui.separator();
                            ui.heading("Buyback Shelf");
                            ui.label("Recently sold items stay here so you can reclaim them.");
                            egui::ScrollArea::vertical()
                                .id_salt(ui.id().with("scroll_trader_buyback"))
                                .max_height(160.0)
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    let mut visible_buyback = 0usize;
                                    for (index, item) in buyback.items.iter().enumerate() {
                                        if !matches_trader_filter(item, tabs.trader_filter) {
                                            continue;
                                        }
                                        visible_buyback += 1;
                                        let quantity = merchant_item_quantity(item, &merchant_counts);
                                        let price = merchant_buyback_price(item, quantity);
                                        let afford = wallet.gold >= price;
                                        if paint_item_buy_card(ui, item, &format!("Rebuy {}g", price), "Buyback", afford) {
                                            pending_buyback = Some(index);
                                        }
                                        draw_trader_item_compare(ui, item, &equip, &ps);
                                        ui.add_space(4.0);
                                    }
                                    if visible_buyback == 0 {
                                        ui.label("No buyback items match this filter.");
                                    }
                                });
                        });
                    });

                    if let Some(index) = pending_sell {
                        if let Some(item) = stash.slots.get_mut(index).and_then(Option::take) {
                            let quantity = merchant_item_quantity(&item, &merchant_counts);
                            wallet.gold += merchant_sell_price(&item, quantity);
                            push_buyback_item(&mut buyback, item);
                        }
                    }

                    if let Some(index) = pending_buy {
                        if let Some(item) = merchant.slots.get(index).and_then(|slot| slot.as_ref()).cloned() {
                            let quantity = merchant_item_quantity(&item, &merchant_counts);
                            let price = merchant_buy_price(&item, quantity);
                            if wallet.gold >= price && bag.try_add(item.clone()) {
                                wallet.gold -= price;
                                merchant.slots[index] = None;
                            }
                        }
                    }

                    if let Some(index) = pending_buyback {
                        if let Some(item) = buyback.items.get(index).cloned() {
                            let quantity = merchant_item_quantity(&item, &merchant_counts);
                            let price = merchant_buyback_price(&item, quantity);
                            if wallet.gold >= price && bag.try_add(item.clone()) {
                                wallet.gold -= price;
                                buyback.items.remove(index);
                            }
                        }
                    }
                }
                StartMenuTab::Character => {
                    ui.heading("Character Preview");
                    ui.label("Preview of currently equipped gear. This reflects the active equipment used by the character in-game.");
                    ui.separator();

                    ui.columns(2, |cols| {
                        cols[0].group(|ui| {
                            ui.set_min_size(egui::vec2(300.0, 380.0));
                            draw_character_paper_doll(ui, &equip);
                        });

                        cols[1].group(|ui| {
                            ui.set_min_size(egui::vec2(300.0, 380.0));
                            ui.heading("Equipped Gear");
                            ui.separator();

                            draw_equipped_item(ui, "Hat", &equip.hat);
                            draw_equipped_item(ui, "Cape", &equip.cape);
                            draw_equipped_item(ui, "Necklace", &equip.necklace);
                            draw_equipped_item(ui, "Shirt", &equip.shirt);
                            draw_equipped_item(ui, "Gloves", &equip.gloves);
                            draw_equipped_item(ui, "Pants", &equip.pants);
                            draw_equipped_item(ui, "Shoes", &equip.shoes);
                            draw_equipped_item(ui, "Main Hand", &equip.mainhand);
                            draw_equipped_item(ui, "Off Hand", &equip.offhand);
                            draw_equipped_item(ui, "Two Handed", &equip.twohand);
                            draw_equipped_item(ui, "Bag", &equip.bag);
                            draw_equipped_item(ui, "Watch", &equip.watch);
                        });
                    });
                }
            });
        });
}

fn merchant_stock_counts(merchant: &MerchantStore, buyback: &MerchantBuyback) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for item in merchant.slots.iter().flatten() {
        *counts.entry(item.merchant_stock_key()).or_insert(0) += 1;
    }
    for item in &buyback.items {
        *counts.entry(item.merchant_stock_key()).or_insert(0) += 1;
    }
    counts
}

fn merchant_item_quantity(item: &Item, counts: &HashMap<String, usize>) -> usize {
    counts.get(&item.merchant_stock_key()).copied().unwrap_or(0)
}

fn merchant_price_multiplier(quantity: usize) -> f32 {
    (1.75 - quantity as f32 * 0.12).clamp(0.55, 1.75)
}

fn merchant_buy_price(item: &Item, quantity: usize) -> i32 {
    (item.base_value() as f32 * merchant_price_multiplier(quantity)).round().max(1.0) as i32
}

fn merchant_sell_price(item: &Item, quantity: usize) -> i32 {
    (item.base_value() as f32 * merchant_price_multiplier(quantity) * 0.58).round().max(1.0) as i32
}

fn merchant_buyback_price(item: &Item, quantity: usize) -> i32 {
    let buy_price = merchant_buy_price(item, quantity);
    let sell_price = merchant_sell_price(item, quantity);
    ((buy_price as f32 * 0.92).round() as i32).max(sell_price + 2)
}

fn merchant_refresh_cost(state: &MerchantState) -> i32 {
    let used = 3_i32.saturating_sub(state.refreshes_remaining as i32);
    40 + used * 18
}

fn push_buyback_item(buyback: &mut MerchantBuyback, item: Item) {
    const BUYBACK_LIMIT: usize = 16;

    buyback.items.insert(0, item);
    if buyback.items.len() > BUYBACK_LIMIT {
        buyback.items.truncate(BUYBACK_LIMIT);
    }
}

fn matches_trader_filter(item: &Item, filter: TraderShelfFilter) -> bool {
    match filter {
        TraderShelfFilter::All => true,
        TraderShelfFilter::Weapons => item.weapon.is_some(),
        TraderShelfFilter::Gear => item.equip_slot.is_some() && item.weapon.is_none(),
        TraderShelfFilter::Consumables => item.consumable.is_some(),
        TraderShelfFilter::Premium => matches!(item.rarity, crate::items::Rarity::UltraRare | crate::items::Rarity::Legendary | crate::items::Rarity::Unique),
    }
}

fn menu_equipment_slots() -> [(&'static str, crate::items::EquipSlot); 12] {
    use crate::items::EquipSlot;

    [
        ("Watch", EquipSlot::Watch),
        ("Necklace", EquipSlot::Necklace),
        ("Gloves", EquipSlot::Gloves),
        ("Shirt", EquipSlot::Shirt),
        ("Pants", EquipSlot::Pants),
        ("Shoes", EquipSlot::Shoes),
        ("Hat", EquipSlot::Hat),
        ("Cape", EquipSlot::Cape),
        ("Main Hand", EquipSlot::MainHand),
        ("Off Hand", EquipSlot::OffHand),
        ("Two Handed", EquipSlot::TwoHanded),
        ("Bag", EquipSlot::Bag),
    ]
}

fn equipped_item_for_slot(equip: &Equipment, slot: crate::items::EquipSlot) -> Option<&Item> {
    use crate::items::EquipSlot;

    match slot {
        EquipSlot::Watch => equip.watch.as_ref(),
        EquipSlot::Necklace => equip.necklace.as_ref(),
        EquipSlot::Gloves => equip.gloves.as_ref(),
        EquipSlot::Shirt => equip.shirt.as_ref(),
        EquipSlot::Pants => equip.pants.as_ref(),
        EquipSlot::Shoes => equip.shoes.as_ref(),
        EquipSlot::Hat => equip.hat.as_ref(),
        EquipSlot::Cape => equip.cape.as_ref(),
        EquipSlot::MainHand => equip.mainhand.as_ref(),
        EquipSlot::OffHand => equip.offhand.as_ref(),
        EquipSlot::TwoHanded => equip.twohand.as_ref(),
        EquipSlot::Bag => equip.bag.as_ref(),
    }
}

fn take_equipped_item(equip: &mut Equipment, slot: crate::items::EquipSlot) -> Option<Item> {
    use crate::items::EquipSlot;

    match slot {
        EquipSlot::Watch => equip.watch.take(),
        EquipSlot::Necklace => equip.necklace.take(),
        EquipSlot::Gloves => equip.gloves.take(),
        EquipSlot::Shirt => equip.shirt.take(),
        EquipSlot::Pants => equip.pants.take(),
        EquipSlot::Shoes => equip.shoes.take(),
        EquipSlot::Hat => equip.hat.take(),
        EquipSlot::Cape => equip.cape.take(),
        EquipSlot::MainHand => equip.mainhand.take(),
        EquipSlot::OffHand => equip.offhand.take(),
        EquipSlot::TwoHanded => equip.twohand.take(),
        EquipSlot::Bag => equip.bag.take(),
    }
}

fn place_item_in_menu_storage(
    item: Item,
    bag: &mut BagGrid,
    stash: &mut Stash,
    prefer_stash: bool,
) -> Result<(), Item> {
    if prefer_stash {
        if let Some(slot) = stash.slots.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(item);
            return Ok(());
        }
        if bag.try_add(item.clone()) {
            return Ok(());
        }
    } else {
        if bag.try_add(item.clone()) {
            return Ok(());
        }
        if let Some(slot) = stash.slots.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(item);
            return Ok(());
        }
    }
    Err(item)
}

fn recompute_menu_stats(ps: &mut PlayerStats, equip: &Equipment) {
    let mut total = ps.base;
    equip.sum_mods_into(&mut total);
    ps.bonus = Stats {
        vigor: total.vigor - ps.base.vigor,
        strength: total.strength - ps.base.strength,
        agility: total.agility - ps.base.agility,
        magic: total.magic - ps.base.magic,
        endurance: total.endurance - ps.base.endurance,
    };
    ps.recompute_limbs();
}

fn equip_menu_item_from_bag(
    index: usize,
    bag: &mut BagGrid,
    stash: &mut Stash,
    equip: &mut Equipment,
    ps: &mut PlayerStats,
) {
    let Some(item) = bag.cells.get_mut(index).and_then(Option::take) else { return; };
    if item.equip_slot.is_none() {
        bag.cells[index] = Some(item);
        return;
    }

    let displaced = equip.equip(item, bag);
    if let Some(displaced_item) = displaced {
        if let Err(returned_item) = place_item_in_menu_storage(displaced_item, bag, stash, true) {
            let _ = bag.try_add(returned_item);
        }
    }

    recompute_menu_stats(ps, equip);
}

fn equip_menu_item_from_stash(
    index: usize,
    bag: &mut BagGrid,
    stash: &mut Stash,
    equip: &mut Equipment,
    ps: &mut PlayerStats,
) {
    let Some(item) = stash.slots.get_mut(index).and_then(Option::take) else { return; };
    if item.equip_slot.is_none() {
        stash.slots[index] = Some(item);
        return;
    }

    let displaced = equip.equip(item, bag);
    if let Some(displaced_item) = displaced {
        if let Err(returned_item) = place_item_in_menu_storage(displaced_item, bag, stash, true) {
            stash.slots[index] = Some(returned_item);
        }
    }

    recompute_menu_stats(ps, equip);
}

fn unequip_menu_slot(
    slot: crate::items::EquipSlot,
    bag: &mut BagGrid,
    stash: &mut Stash,
    equip: &mut Equipment,
    ps: &mut PlayerStats,
) {
    let Some(item) = take_equipped_item(equip, slot) else { return; };
    if let Err(returned_item) = place_item_in_menu_storage(item, bag, stash, false) {
        let _ = equip.equip(returned_item, bag);
        return;
    }
    recompute_menu_stats(ps, equip);
}

fn rarity_color32(item: &Item) -> egui::Color32 {
    let color = item.rarity.color().to_srgba();
    egui::Color32::from_rgb(
        (color.red * 255.0) as u8,
        (color.green * 255.0) as u8,
        (color.blue * 255.0) as u8,
    )
}

fn draw_equipped_item(ui: &mut egui::Ui, label: &str, item: &Option<Item>) {
    ui.group(|ui| {
        ui.set_min_width(220.0);
        ui.strong(label);
        if let Some(item) = item {
            ui.horizontal(|ui| {
                paint_item_badge(ui, item, egui::vec2(28.0, 28.0));
                ui.vertical(|ui| {
                    ui.colored_label(rarity_color32(item), &item.name);
                    ui.small(item_descriptor(item));
                });
            });
        } else {
            ui.small("Empty");
        }
    });
    ui.add_space(4.0);
}

fn draw_character_paper_doll(ui: &mut egui::Ui, equip: &Equipment) {
    let desired = egui::vec2(268.0, 356.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 18.0, egui::Color32::from_rgb(18, 22, 29));
    painter.rect_stroke(
        rect.shrink(1.0),
        18.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
        egui::StrokeKind::Outside,
    );
    painter.circle_filled(
        egui::pos2(rect.center().x, rect.top() + 122.0),
        112.0,
        egui::Color32::from_rgba_unmultiplied(82, 106, 136, 28),
    );
    painter.circle_stroke(
        egui::pos2(rect.center().x, rect.top() + 122.0),
        112.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(190, 214, 240, 26)),
    );

    let center_x = rect.center().x;
    let head_center = egui::pos2(center_x, rect.top() + 70.0);
    let body_top = rect.top() + 110.0;
    let body_bottom = rect.bottom() - 64.0;
    let skin = egui::Color32::from_rgb(223, 191, 164);
    let cloth = egui::Color32::from_rgb(60, 82, 112);
    let trim = egui::Color32::from_rgb(174, 194, 222);
    let leather = egui::Color32::from_rgb(92, 62, 40);
    let hair = egui::Color32::from_rgb(42, 29, 22);

    painter.circle_filled(egui::pos2(center_x, body_top + 92.0), 56.0, egui::Color32::from_rgba_unmultiplied(12, 14, 16, 40));
    painter.circle_filled(head_center, 25.0, skin);
    painter.circle_filled(egui::pos2(center_x, head_center.y - 10.0), 20.0, hair);
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(center_x, head_center.y - 18.0), egui::vec2(28.0, 8.0)),
        4.0,
        hair,
    );
    painter.circle_filled(egui::pos2(center_x - 8.0, head_center.y - 2.0), 2.0, egui::Color32::from_rgb(34, 28, 24));
    painter.circle_filled(egui::pos2(center_x + 8.0, head_center.y - 2.0), 2.0, egui::Color32::from_rgb(34, 28, 24));
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(center_x, (body_top + body_bottom) * 0.5), egui::vec2(60.0, 132.0)),
        20.0,
        cloth,
    );
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(center_x, body_top + 46.0), egui::vec2(42.0, 74.0)),
        12.0,
        egui::Color32::from_rgba_unmultiplied(228, 236, 245, 28),
    );
    painter.line_segment(
        [egui::pos2(center_x - 50.0, body_top + 30.0), egui::pos2(center_x + 50.0, body_top + 30.0)],
        egui::Stroke::new(12.0, cloth),
    );
    painter.line_segment(
        [egui::pos2(center_x - 16.0, body_bottom - 14.0), egui::pos2(center_x - 28.0, rect.bottom() - 22.0)],
        egui::Stroke::new(12.0, cloth),
    );
    painter.line_segment(
        [egui::pos2(center_x + 16.0, body_bottom - 14.0), egui::pos2(center_x + 28.0, rect.bottom() - 22.0)],
        egui::Stroke::new(12.0, cloth),
    );
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(center_x, body_top + 42.0), egui::vec2(46.0, 10.0)),
        5.0,
        trim,
    );
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(center_x, body_bottom - 8.0), egui::vec2(58.0, 8.0)),
        4.0,
        leather,
    );
    painter.line_segment(
        [egui::pos2(center_x - 38.0, body_top + 34.0), egui::pos2(center_x - 58.0, body_top + 108.0)],
        egui::Stroke::new(8.0, cloth),
    );
    painter.line_segment(
        [egui::pos2(center_x + 38.0, body_top + 34.0), egui::pos2(center_x + 58.0, body_top + 108.0)],
        egui::Stroke::new(8.0, cloth),
    );

    let slot_positions = [
        (equip.hat.as_ref(), egui::pos2(center_x, rect.top() + 24.0)),
        (equip.necklace.as_ref(), egui::pos2(center_x, body_top + 8.0)),
        (equip.cape.as_ref(), egui::pos2(center_x + 44.0, body_top + 56.0)),
        (equip.gloves.as_ref(), egui::pos2(center_x - 54.0, body_top + 34.0)),
        (equip.shirt.as_ref(), egui::pos2(center_x, body_top + 52.0)),
        (equip.pants.as_ref(), egui::pos2(center_x, body_bottom - 26.0)),
        (equip.shoes.as_ref(), egui::pos2(center_x - 22.0, rect.bottom() - 18.0)),
        (equip.mainhand.as_ref().or(equip.twohand.as_ref()), egui::pos2(center_x + 62.0, body_top + 58.0)),
        (equip.offhand.as_ref(), egui::pos2(center_x - 62.0, body_top + 58.0)),
        (equip.bag.as_ref(), egui::pos2(center_x - 46.0, body_bottom - 4.0)),
        (equip.watch.as_ref(), egui::pos2(center_x + 50.0, body_top + 30.0)),
    ];

    for (item, center) in slot_positions {
        let badge_rect = egui::Rect::from_center_size(center, egui::vec2(26.0, 26.0));
        painter.rect_filled(badge_rect, 8.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 16));
        if let Some(item) = item {
            let mini = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(badge_rect)
                    .layout(*ui.layout()),
            );
            let mut mini = mini;
            paint_item_badge(&mut mini, item, badge_rect.size());
        }
    }
}

fn paint_item_badge(ui: &mut egui::Ui, item: &Item, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let (primary, secondary, accent) = item_palette(item);
    painter.rect_filled(rect, 8.0, primary);
    painter.rect_stroke(
        rect.shrink(0.5),
        8.0,
        egui::Stroke::new(1.0, accent),
        egui::StrokeKind::Outside,
    );
    paint_item_glyph(&painter, rect.shrink(4.0), item, secondary, accent);
}

fn paint_item_glyph(
    painter: &egui::Painter,
    rect: egui::Rect,
    item: &Item,
    secondary: egui::Color32,
    accent: egui::Color32,
) {
    use crate::items::{EquipSlot, WeaponKind};

    match item.weapon {
        Some(WeaponKind::TwoHandedSword) | Some(WeaponKind::LongSword) => {
            painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(4.0, rect.height() * 0.7)), 2.0, secondary);
            painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.center().y - 6.0), egui::vec2(rect.width() * 0.42, 4.0)), 2.0, accent);
        }
        Some(WeaponKind::Dagger) | Some(WeaponKind::ShortSword) => {
            painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(3.0, rect.height() * 0.52)), 2.0, secondary);
            painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.center().y + 6.0), egui::vec2(rect.width() * 0.24, 4.0)), 2.0, accent);
        }
        Some(WeaponKind::DoubleAxe) => {
            painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(4.0, rect.height() * 0.72)), 2.0, secondary);
            painter.circle_filled(egui::pos2(rect.center().x - 8.0, rect.top() + rect.height() * 0.34), 7.0, accent);
            painter.circle_filled(egui::pos2(rect.center().x + 8.0, rect.top() + rect.height() * 0.34), 7.0, accent);
        }
        Some(WeaponKind::Hatchet) => {
            painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(4.0, rect.height() * 0.66)), 2.0, secondary);
            painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x + 6.0, rect.top() + rect.height() * 0.34), egui::vec2(rect.width() * 0.26, rect.height() * 0.18)), 3.0, accent);
        }
        Some(WeaponKind::Scythe) => {
            painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(3.0, rect.height() * 0.82)), 2.0, secondary);
            painter.circle_stroke(egui::pos2(rect.center().x + 10.0, rect.top() + rect.height() * 0.26), 10.0, egui::Stroke::new(3.0, accent));
        }
        Some(WeaponKind::GiantHammer) => {
            painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.center().y + 3.0), egui::vec2(4.0, rect.height() * 0.62)), 2.0, secondary);
            painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + rect.height() * 0.28), egui::vec2(rect.width() * 0.42, rect.height() * 0.24)), 4.0, accent);
        }
        Some(WeaponKind::MagicStaff) => {
            painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(4.0, rect.height() * 0.84)), 2.0, secondary);
            painter.circle_filled(egui::pos2(rect.center().x, rect.top() + rect.height() * 0.18), rect.width() * 0.14, accent);
        }
        Some(WeaponKind::Lantern) => {
            let body = egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.38, rect.height() * 0.42));
            painter.rect_filled(body, 4.0, secondary);
            painter.circle_stroke(egui::pos2(rect.center().x, body.top() - 2.0), rect.width() * 0.12, egui::Stroke::new(2.0, accent));
        }
        Some(WeaponKind::CrystalBall) => {
            painter.circle_filled(rect.center(), rect.width() * 0.22, accent);
            painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom() - 6.0), egui::vec2(rect.width() * 0.28, 5.0)), 3.0, secondary);
        }
        Some(WeaponKind::Book) => {
            painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.46, rect.height() * 0.52)), 3.0, secondary);
            painter.line_segment([egui::pos2(rect.center().x, rect.top() + rect.height() * 0.24), egui::pos2(rect.center().x, rect.bottom() - rect.height() * 0.24)], egui::Stroke::new(2.0, accent));
        }
        None => match item.equip_slot {
            Some(EquipSlot::Hat) => {
                painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.56, rect.height() * 0.28)), 6.0, secondary);
            }
            Some(EquipSlot::Cape) => {
                painter.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.left() + rect.width() * 0.28, rect.top() + 4.0), egui::pos2(rect.right() - rect.width() * 0.28, rect.bottom() - 3.0)), 5.0, secondary);
            }
            Some(EquipSlot::Necklace) => {
                painter.circle_stroke(rect.center(), rect.width() * 0.2, egui::Stroke::new(2.0, secondary));
            }
            Some(EquipSlot::Gloves) => {
                painter.circle_filled(egui::pos2(rect.center().x - 6.0, rect.center().y), 5.0, secondary);
                painter.circle_filled(egui::pos2(rect.center().x + 6.0, rect.center().y), 5.0, accent);
            }
            Some(EquipSlot::Shirt) => {
                painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.46, rect.height() * 0.5)), 4.0, secondary);
            }
            Some(EquipSlot::Pants) => {
                painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x - 5.0, rect.center().y), egui::vec2(rect.width() * 0.16, rect.height() * 0.52)), 3.0, secondary);
                painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x + 5.0, rect.center().y), egui::vec2(rect.width() * 0.16, rect.height() * 0.52)), 3.0, accent);
            }
            Some(EquipSlot::Shoes) => {
                painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x - 6.0, rect.bottom() - 6.0), egui::vec2(10.0, 6.0)), 3.0, secondary);
                painter.rect_filled(egui::Rect::from_center_size(egui::pos2(rect.center().x + 6.0, rect.bottom() - 6.0), egui::vec2(10.0, 6.0)), 3.0, accent);
            }
            Some(EquipSlot::Bag) => {
                painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.46, rect.height() * 0.52)), 5.0, secondary);
                painter.circle_stroke(egui::pos2(rect.center().x, rect.top() + rect.height() * 0.34), rect.width() * 0.14, egui::Stroke::new(2.0, accent));
            }
            Some(EquipSlot::Watch) => {
                painter.circle_filled(rect.center(), rect.width() * 0.16, secondary);
                painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.46, 4.0)), 2.0, accent);
            }
            _ => {
                painter.rect_filled(egui::Rect::from_center_size(rect.center(), egui::vec2(rect.width() * 0.38, rect.height() * 0.38)), 4.0, secondary);
            }
        },
    }
}

fn paint_bag_slot_button(
    ui: &mut egui::Ui,
    item: Option<&Item>,
    selected: bool,
) -> bool {
    let size = egui::vec2(66.0, 66.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter_at(rect);
    let frame_fill = if selected {
        egui::Color32::from_rgb(46, 70, 98)
    } else {
        egui::Color32::from_rgb(24, 28, 34)
    };
    let stroke = if selected {
        egui::Stroke::new(2.0, egui::Color32::from_rgb(180, 212, 242))
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 22))
    };
    painter.rect_filled(rect, 10.0, frame_fill);
    painter.rect_stroke(rect.shrink(0.5), 10.0, stroke, egui::StrokeKind::Outside);

    if let Some(item) = item {
        let badge_rect = egui::Rect::from_center_size(rect.center_top() + egui::vec2(0.0, 22.0), egui::vec2(28.0, 28.0));
        let mut badge_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(badge_rect)
                .layout(*ui.layout()),
        );
        paint_item_badge(&mut badge_ui, item, badge_rect.size());
        let short_name: String = item.name.chars().take(6).collect();
        painter.text(
            egui::pos2(rect.center().x, rect.bottom() - 12.0),
            egui::Align2::CENTER_CENTER,
            short_name,
            egui::FontId::proportional(11.0),
            egui::Color32::from_rgb(226, 232, 240),
        );
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "•",
            egui::FontId::proportional(22.0),
            egui::Color32::from_rgb(92, 100, 112),
        );
    }

    response.clicked()
}

fn paint_stat_chip(ui: &mut egui::Ui, label: &str, total: i32, bonus: i32) {
    let fill = if bonus > 0 {
        egui::Color32::from_rgb(28, 56, 44)
    } else if bonus < 0 {
        egui::Color32::from_rgb(66, 28, 34)
    } else {
        egui::Color32::from_rgb(28, 34, 40)
    };
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 18)))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.label(format!("{} {} ({:+})", label, total, bonus));
        });
}

fn paint_trader_filter_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let width = (label.len() as f32 * 7.8).clamp(74.0, 132.0);
    let size = egui::vec2(width, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter_at(rect);
    let hovered = response.hovered();

    let fill = if selected {
        egui::Color32::from_rgb(213, 177, 78)
    } else if hovered {
        egui::Color32::from_rgb(54, 60, 68)
    } else {
        egui::Color32::from_rgb(34, 38, 44)
    };
    let stroke = if selected {
        egui::Stroke::new(1.4, egui::Color32::from_rgb(248, 223, 142))
    } else {
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 20))
    };
    let text_color = if selected {
        egui::Color32::from_rgb(28, 21, 9)
    } else {
        egui::Color32::from_rgb(212, 218, 228)
    };

    painter.rect_filled(rect, 9.0, fill);
    painter.rect_stroke(rect.shrink(0.5), 9.0, stroke, egui::StrokeKind::Outside);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0),
        text_color,
    );

    response.clicked()
}

fn paint_item_action_card(
    ui: &mut egui::Ui,
    item: &Item,
    primary_action: &str,
    secondary_action: Option<&str>,
) -> (bool, bool) {
    let mut primary_clicked = false;
    let mut secondary_clicked = false;

    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                paint_item_badge(ui, item, egui::vec2(34.0, 34.0));
                ui.vertical(|ui| {
                    ui.colored_label(rarity_color32(item), &item.name);
                    ui.small(item_descriptor(item));
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button(primary_action).clicked() {
                    primary_clicked = true;
                }
                if let Some(action) = secondary_action {
                    if ui.button(action).clicked() {
                        secondary_clicked = true;
                    }
                }
            });
        });

    ui.add_space(4.0);
    (primary_clicked, secondary_clicked)
}

fn paint_item_buy_card(
    ui: &mut egui::Ui,
    item: &Item,
    action_label: &str,
    detail: &str,
    enabled: bool,
) -> bool {
    let mut clicked = false;

    egui::Frame::group(ui.style())
        .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                paint_item_badge(ui, item, egui::vec2(34.0, 34.0));
                ui.vertical(|ui| {
                    ui.colored_label(rarity_color32(item), &item.name);
                    ui.small(format!("{}  •  {}", item_descriptor(item), detail));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(enabled, egui::Button::new(action_label)).clicked() {
                        clicked = true;
                    }
                });
            });
        });

    clicked
}

fn item_descriptor(item: &Item) -> String {
    match (item.weapon, item.equip_slot, item.consumable) {
        (Some(weapon), _, _) => format!("{:?}", weapon),
        (_, Some(slot), _) => format!("{:?}", slot),
        (_, _, Some(consumable)) => format!("{:?}", consumable),
        _ => "Item".to_owned(),
    }
}

fn item_palette(item: &Item) -> (egui::Color32, egui::Color32, egui::Color32) {
    let seed = item_visual_seed_ui(item);
    let base = item.rarity.color().to_srgba();
    let accent = ((seed >> 6) & 0xff) as f32 / 255.0;
    let primary = egui::Color32::from_rgb(
        ((base.red * 190.0) + 26.0 + accent * 22.0).clamp(0.0, 255.0) as u8,
        ((base.green * 182.0) + 20.0 + (1.0 - accent) * 18.0).clamp(0.0, 255.0) as u8,
        ((base.blue * 176.0) + 24.0 + accent * 12.0).clamp(0.0, 255.0) as u8,
    );
    let secondary = egui::Color32::from_rgb(
        ((base.red * 90.0) + 110.0).clamp(0.0, 255.0) as u8,
        ((base.green * 82.0) + 104.0).clamp(0.0, 255.0) as u8,
        ((base.blue * 84.0) + 112.0).clamp(0.0, 255.0) as u8,
    );
    let accent_color = egui::Color32::from_rgb(
        ((base.red * 120.0) + 100.0 + accent * 34.0).clamp(0.0, 255.0) as u8,
        ((base.green * 104.0) + 90.0).clamp(0.0, 255.0) as u8,
        ((base.blue * 126.0) + 86.0 + (1.0 - accent) * 26.0).clamp(0.0, 255.0) as u8,
    );
    (primary, secondary, accent_color)
}

fn item_visual_seed_ui(item: &Item) -> u32 {
    let mut hash = 0x811C9DC5u32;
    for byte in item.name.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash ^ (item.base_value() as u32)
}

/// Inventory & equipment UI, and chest UI when open
pub fn inventory_ui(
    mut ctxs: EguiContexts,
    mut sel: ResMut<UISelection>,
    mut bag: ResMut<BagGrid>,
    mut equip: ResMut<Equipment>,
    mut ps: ResMut<PlayerStats>,
    open: Res<OpenChest>,                         // <--- NEW
    flags: Res<UIFlags>,
    mut chests: Query<&mut Chest>,
) {
    let Ok(ctx) = ctxs.ctx_mut() else { return; };

    if flags.inventory_open {
        let total = Stats {
            vigor: ps.base.vigor + ps.bonus.vigor,
            strength: ps.base.strength + ps.bonus.strength,
            agility: ps.base.agility + ps.bonus.agility,
            magic: ps.base.magic + ps.bonus.magic,
            endurance: ps.base.endurance + ps.bonus.endurance,
        };
        egui::Window::new("Inventory")
            .id(egui::Id::new("window.inventory")) // Window can take an explicit Id
            .default_size(egui::vec2(980.0, 620.0))
            .default_pos(egui::pos2(24.0, 28.0))
            .show(ctx, |ui| {
                let BagGrid { w, h, .. } = *bag;
                ui.horizontal(|ui| {
                    paint_coin_icon(ui, 18.0);
                    ui.heading(format!("Field Bag {}x{}", w, h));
                });
                ui.label("Select an item to inspect it, then equip it directly from the bag. This layout mirrors the upgraded start-menu character screen.");
                ui.separator();

                let mut try_equip_selected = false;

                ui.columns(2, |cols| {
                    cols[0].group(|ui| {
                        ui.set_min_size(egui::vec2(440.0, 540.0));
                        ui.heading("Bag Grid");
                        ui.small(format!("{} occupied slots", bag.cells.iter().filter(|cell| cell.is_some()).count()));
                        ui.separator();

                        egui::ScrollArea::both()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                egui::Grid::new(egui::Id::new("grid.bag.cards"))
                                    .spacing(egui::vec2(6.0, 6.0))
                                    .show(ui, |ui| {
                                        for y in 0..h {
                                            for x in 0..w {
                                                ui.push_id(("bag_card", x, y), |ui| {
                                                    let idx = (y as usize) * (w as usize) + (x as usize);
                                                    let item = bag.cells[idx].as_ref();
                                                    let selected = sel.selected_bag_cell == Some((x, y));
                                                    if paint_bag_slot_button(ui, item, selected) {
                                                        sel.selected_bag_cell = Some((x, y));
                                                    }
                                                });
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });

                        ui.separator();
                        ui.heading("Selected Item");
                        if let Some((x, y)) = sel.selected_bag_cell {
                            let idx = (y as usize) * (w as usize) + (x as usize);
                            if let Some(item) = bag.cells.get(idx).and_then(|cell| cell.as_ref()) {
                                ui.horizontal(|ui| {
                                    paint_item_badge(ui, item, egui::vec2(42.0, 42.0));
                                    ui.vertical(|ui| {
                                        ui.colored_label(rarity_color32(item), &item.name);
                                        ui.small(item_descriptor(item));
                                    });
                                });
                                draw_item_summary(ui, item, &ps);
                                if item.equip_slot.is_some() && ui.button("Equip selected").clicked() {
                                    try_equip_selected = true;
                                }
                            } else {
                                ui.label("No item selected.");
                            }
                        } else {
                            ui.label("Select a bag slot to inspect it.");
                        }
                    });

                    cols[1].group(|ui| {
                        ui.set_min_size(egui::vec2(470.0, 540.0));
                        ui.heading("Character Readout");
                        ui.separator();
                        ui.columns(2, |inner| {
                            inner[0].vertical(|ui| {
                                draw_character_paper_doll(ui, &equip);
                            });
                            inner[1].vertical(|ui| {
                                ui.heading("Combat Preview");
                                draw_equipped_weapon_summary(ui, "Primary", equip.twohand.as_ref().or(equip.mainhand.as_ref()), WeaponSlot::Primary, &ps);
                                draw_equipped_weapon_summary(ui, "Secondary", equip.offhand.as_ref(), WeaponSlot::Secondary, &ps);
                                ui.separator();
                                ui.heading("Stats");
                                ui.horizontal_wrapped(|ui| {
                                    paint_stat_chip(ui, "VIG", total.vigor, ps.bonus.vigor);
                                    paint_stat_chip(ui, "STR", total.strength, ps.bonus.strength);
                                    paint_stat_chip(ui, "AGI", total.agility, ps.bonus.agility);
                                    paint_stat_chip(ui, "MAG", total.magic, ps.bonus.magic);
                                    paint_stat_chip(ui, "END", total.endurance, ps.bonus.endurance);
                                });
                                ui.separator();
                                ui.heading("Equipped");
                                draw_equipped_item(ui, "Hat", &equip.hat);
                                draw_equipped_item(ui, "Cape", &equip.cape);
                                draw_equipped_item(ui, "Shirt", &equip.shirt);
                                draw_equipped_item(ui, "Pants", &equip.pants);
                                draw_equipped_item(ui, "Bag", &equip.bag);
                            });
                        });
                    });
                });

                if try_equip_selected {
                    if let Some((x,y)) = sel.selected_bag_cell.take() {
                        if let Some(item) = bag.remove_at(x,y) {
                            if let Some(swapped) = equip.equip(item, &mut bag) {
                                let _ = bag.try_add(swapped);
                            }
                            let mut tot = ps.base;
                            equip.sum_mods_into(&mut tot);
                            ps.bonus = Stats {
                                vigor:     tot.vigor - ps.base.vigor,
                                strength:  tot.strength - ps.base.strength,
                                agility:   tot.agility - ps.base.agility,
                                magic:     tot.magic - ps.base.magic,
                                endurance: tot.endurance - ps.base.endurance,
                            };
                            ps.recompute_limbs();
                        }
                    }
                }
            });
    }

    if let Some(target) = open.0 {
        if let Ok(mut chest) = chests.get_mut(target) {
            let chest_win_id = egui::Id::new(("window.chest", target));
            egui::Window::new("Chest")
                .id(chest_win_id)
                .default_size(egui::vec2(420.0, 470.0))
                .default_pos(egui::pos2(420.0, 40.0))
                .show(ctx, |ui| {
                    ui.heading(format!("{} Chest", chest_tier_name(chest.tier)));
                    if chest.gold > 0 {
                        ui.horizontal(|ui| {
                            paint_coin_icon(ui, 15.0);
                            ui.label(format!("Chest gold: {}g", chest.gold));
                        });
                    } else {
                        ui.label("Chest gold already claimed.");
                    }
                    ui.separator();
                    ui.label("Take items directly from the chest. Higher tier chests carry stronger visual profiles and denser loot.");
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_salt(ui.id().with("scroll_items"))
                        .show(ui, |ui| {
                            for (i, it) in chest.items.iter().enumerate() {
                                ui.push_id(("btn_take_item", i), |ui| {
                                    if paint_item_action_card(ui, it, "Take", None).0 {
                                        sel.selected_chest_idx = Some(i);
                                    }
                                });
                            }
                            if chest.items.is_empty() {
                                ui.label("Chest is empty.");
                            }
                        });

                    if let Some(idx) = sel.selected_chest_idx.take() {
                        if let Some(it) = chest.items.get(idx).cloned() {
                            if bag.try_add(it) {
                                chest.items.remove(idx);
                            }
                        }
                    }
                });
        }
    }
}

pub fn pause_menu_ui(
    mut ctxs: EguiContexts,
    mut flags: ResMut<UIFlags>,
    mut camera_mode: ResMut<CameraModeSettings>,
    mut exit: MessageWriter<AppExit>,
    mut next: ResMut<NextState<AppState>>,
) {
    if !flags.pause_menu_open { return; }

    let Ok(ctx) = ctxs.ctx_mut() else { return; };
    egui::Window::new("Pause")
        .id(egui::Id::new("window.pause"))
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                if ui.button("Return to Menu").clicked() {
                    flags.pause_menu_open = false;
                    flags.pause_settings_open = false;
                    next.set(AppState::Menu);
                }
                if ui.button("Settings").clicked() {
                    flags.pause_settings_open = !flags.pause_settings_open;
                }
                if flags.pause_settings_open {
                    ui.separator();
                    ui.group(|ui| {
                        ui.heading("Settings");
                        ui.checkbox(&mut camera_mode.third_person_enabled, "Third-person mode");
                        ui.checkbox(&mut camera_mode.collision_enabled, "Clamp camera against walls");
                        ui.small("Hotkeys: V toggles third-person, C swaps shoulders.");
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "Current shoulder: {}",
                                if camera_mode.shoulder_side >= 0.0 { "Right" } else { "Left" }
                            ));
                            if ui.button("Swap shoulder").clicked() {
                                camera_mode.shoulder_side *= -1.0;
                                if camera_mode.shoulder_side == 0.0 {
                                    camera_mode.shoulder_side = 1.0;
                                }
                            }
                        });
                        ui.add(
                            egui::Slider::new(&mut camera_mode.shoulder_offset, 0.0..=0.9)
                                .text("Shoulder offset")
                                .step_by(0.01),
                        );
                        ui.add(
                            egui::Slider::new(&mut camera_mode.follow_distance, 1.6..=4.2)
                                .text("Camera distance")
                                .step_by(0.01),
                        );
                        ui.add(
                            egui::Slider::new(&mut camera_mode.camera_height, 0.15..=0.95)
                                .text("Camera height")
                                .step_by(0.01),
                        );
                    });
                }
                if ui.button("Exit Game").clicked() {
                    exit.write(AppExit::Success);
                }
            });
        });
}

pub fn stamina_hud_ui(
    mut ctxs: EguiContexts,
    flags: Res<UIFlags>,
    motion_q: Query<&PlayerMotion, With<Player>>,
) {
    let Ok(ctx) = ctxs.ctx_mut() else { return; };
    let Ok(motion) = motion_q.single() else { return; };

    let hud_attention = ((1.0 - motion.sprint_stamina) * 1.8
        + motion.sprint_amount * 1.15
        + motion.move_amount * 0.22
        + if flags.pause_menu_open { 0.35 } else { 0.0 })
        .clamp(0.0, 1.0);
    let hud_alpha = (0.06 + hud_attention * 0.94).clamp(0.06, 1.0);
    if hud_alpha <= 0.065 && motion.sprint_stamina > 0.995 && motion.sprint_amount < 0.02 {
        return;
    }
    let alpha_u8 = (hud_alpha * 255.0).round() as u8;
    let soft_alpha_u8 = (hud_alpha * 180.0).round() as u8;

    let tint = |r: u8, g: u8, b: u8, alpha: u8| egui::Color32::from_rgba_unmultiplied(r, g, b, alpha);

    let bar_color = if motion.sprint_stamina < 0.2 {
        tint(190, 78, 66, alpha_u8)
    } else if motion.sprint_amount > 0.2 {
        tint(160, 204, 92, alpha_u8)
    } else {
        tint(115, 157, 92, alpha_u8)
    };
    let status = if motion.sprint_stamina < 0.08 {
        "Winded"
    } else if motion.sprint_amount > 0.25 {
        "Sprinting"
    } else {
        "Stamina"
    };
    let frame_fill = if flags.pause_menu_open {
        tint(12, 16, 12, (hud_alpha * 190.0).round() as u8)
    } else {
        tint(12, 16, 12, (hud_alpha * 210.0).round() as u8)
    };

    egui::Area::new(egui::Id::new("hud.stamina"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 16.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(frame_fill)
                .stroke(egui::Stroke::new(1.0, tint(170, 196, 150, soft_alpha_u8)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(172.0);
                    ui.label(egui::RichText::new(status).strong().color(tint(226, 236, 209, alpha_u8)));
                    ui.add(
                        egui::ProgressBar::new(motion.sprint_stamina)
                            .desired_width(152.0)
                            .fill(bar_color)
                            .text(format!("{:>3}%", (motion.sprint_stamina * 100.0).round() as i32)),
                    );
                });
        });
}

pub fn player_health_hud_ui(
    mut ctxs: EguiContexts,
    flags: Res<UIFlags>,
    health_q: Query<&Health, With<Player>>,
) {
    let Ok(ctx) = ctxs.ctx_mut() else { return; };
    let Ok(health) = health_q.single() else { return; };
    let value = if health.max > 0.0 { (health.hp / health.max).clamp(0.0, 1.0) } else { 0.0 };
    let pulse = (1.0 - value).clamp(0.0, 1.0);
    let alpha = if flags.pause_menu_open { 215 } else { 240 };
    let fill = if value < 0.22 {
        egui::Color32::from_rgba_unmultiplied(196, 58, 58, alpha)
    } else {
        egui::Color32::from_rgba_unmultiplied(170, 38, 52, alpha)
    };
    let frame_fill = egui::Color32::from_rgba_unmultiplied(18, 10, 12, (150.0 + pulse * 70.0).round() as u8);

    egui::Area::new(egui::Id::new("hud.health"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 82.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(frame_fill)
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(225, 180, 180, 190)))
                .corner_radius(egui::CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(172.0);
                    ui.label(egui::RichText::new("Health").strong().color(egui::Color32::from_rgb(245, 226, 226)));
                    ui.add(
                        egui::ProgressBar::new(value)
                            .desired_width(152.0)
                            .fill(fill)
                            .text(format!("{:.0}/{:.0}", health.hp.max(0.0), health.max.max(1.0))),
                    );
                });
        });
}

pub fn enemy_health_bars_ui(
    mut ctxs: EguiContexts,
    rapier_ctx: ReadRapierContext,
    equipment: Res<Equipment>,
    camera_q: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    enemies: Query<(Entity, &Health, &GlobalTransform), With<EnemyHealthBarAnchor>>,
) {
    let Ok(ctx) = ctxs.ctx_mut() else { return; };
    let Ok(rapier) = rapier_ctx.single() else { return; };
    let Ok((camera, cam_tf)) = camera_q.single() else { return; };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("enemy.healthbars")));
    let camera_pos = cam_tf.translation();
    let crystal_ball_equipped = [&equipment.mainhand, &equipment.offhand, &equipment.twohand]
        .into_iter()
        .flatten()
        .any(|item| item.weapon == Some(crate::items::WeaponKind::CrystalBall));

    for (enemy_e, health, world_tf) in &enemies {
        if health.hp <= 0.0 || health.max <= 0.0 {
            continue;
        }
        let world_pos = world_tf.translation() + Vec3::new(0.0, 1.45, 0.0);
        let to_target = world_pos - camera_pos;
        let target_distance = to_target.length();
        if target_distance <= 0.001 {
            continue;
        }
        let ray_dir = to_target / target_distance;
        let ray_start = camera_pos + ray_dir * 0.35;
        let ray_length = (target_distance - 0.35).max(0.05);
        if !crystal_ball_equipped {
            if let Some((hit, _)) = rapier.cast_ray(ray_start, ray_dir, ray_length, true, QueryFilter::default()) {
                if hit != enemy_e {
                    continue;
                }
            }
        }
        let Ok(screen) = camera.world_to_viewport(cam_tf, world_pos) else { continue; };
        let fill = (health.hp / health.max).clamp(0.0, 1.0);
        let size = egui::vec2(56.0, 7.0);
        let rect = egui::Rect::from_center_size(egui::pos2(screen.x, screen.y), size);
        painter.rect_filled(rect.expand2(egui::vec2(2.0, 2.0)), 4.0, egui::Color32::from_rgba_unmultiplied(8, 10, 12, 180));
        painter.rect_filled(rect, 3.0, egui::Color32::from_rgba_unmultiplied(38, 16, 18, 220));
        let fill_rect = egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + rect.width() * fill, rect.max.y));
        painter.rect_filled(fill_rect, 3.0, egui::Color32::from_rgba_unmultiplied(184, 54, 66, 240));
    }
}

pub fn damage_popup_ui(
    mut ctxs: EguiContexts,
    camera_q: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    popups: Query<(&DamagePopup, &GlobalTransform)>,
) {
    let Ok(ctx) = ctxs.ctx_mut() else { return; };
    let Ok((camera, cam_tf)) = camera_q.single() else { return; };
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("damage.popups")));

    for (popup, world_tf) in &popups {
        let Ok(screen) = camera.world_to_viewport(cam_tf, world_tf.translation()) else { continue; };
        let alpha = (1.0 - popup.lifetime.fraction()).clamp(0.0, 1.0);
        let (color, shadow, text) = if popup.positive {
            (
                egui::Color32::from_rgba_unmultiplied(246, 224, 132, (alpha * 255.0) as u8),
                egui::Color32::from_rgba_unmultiplied(34, 22, 5, (alpha * 180.0) as u8),
                format!("+{:.0}g", popup.amount.max(0.0)),
            )
        } else {
            (
                egui::Color32::from_rgba_unmultiplied(255, 214, 120, (alpha * 255.0) as u8),
                egui::Color32::from_rgba_unmultiplied(20, 8, 8, (alpha * 180.0) as u8),
                format!("-{:.0}", popup.amount.max(0.0)),
            )
        };
        let pos = egui::pos2(screen.x, screen.y);
        let font = egui::FontId::proportional(18.0);
        painter.text(pos + egui::vec2(1.5, 1.5), egui::Align2::CENTER_CENTER, &text, font.clone(), shadow);
        painter.text(pos, egui::Align2::CENTER_CENTER, text, font, color);
    }
}

pub fn spell_menu_ui(
    mut ctxs: EguiContexts,
    flags: Res<UIFlags>,
    mut active: ResMut<ActiveSpell>,
    book: Res<Spellbook>,
) {
    if !flags.spell_menu_open { return; }

    let Ok(ctx) = ctxs.ctx_mut() else { return; };
    egui::Window::new("Spells")
        .id(egui::Id::new("window.spells"))
        .default_size(egui::vec2(260.0, 220.0))
        .default_pos(egui::pos2(968.0, 42.0))
        .show(ctx, |ui| {
            ui.label("Select a spell, then Left Click to cast.");
            ui.separator();

            let mut spell_button = |ui: &mut egui::Ui, s: Spell, label: &str| {
                let have = *book.charges.get(&s).unwrap_or(&0);
                let btn = ui.button(format!("{label}  ({have})"));
                if btn.clicked() {
                    active.selected = Some(s);
                }
            };

            ui.horizontal(|ui| {
                spell_button(ui, Spell::Fireball,  "Fireball");
                spell_button(ui, Spell::WaterGun,  "Water Gun");
            });
            ui.horizontal(|ui| {
                spell_button(ui, Spell::Zap,       "Zap");
                spell_button(ui, Spell::WindSlash, "Wind Slash");
            });
            ui.horizontal(|ui| {
                spell_button(ui, Spell::LightHeal, "Light Heal");
            });

            if let Some(sel) = active.selected {
                ui.separator();
                ui.label(format!("Selected: {:?}", sel));
            }
        });
}

fn draw_equipped_weapon_summary(
    ui: &mut egui::Ui,
    label: &str,
    item: Option<&Item>,
    slot: WeaponSlot,
    player_stats: &PlayerStats,
) {
    ui.group(|ui| {
        ui.strong(label);
        match item {
            Some(item) => {
                ui.label(format!("{} ({:?})", item.name, item.rarity));
                draw_weapon_preview_lines(ui, item, slot, player_stats);
            }
            None => {
                ui.label("None equipped");
            }
        }
    });
}

fn draw_item_summary(ui: &mut egui::Ui, item: &Item, player_stats: &PlayerStats) {
    ui.label(format!("{} ({:?})", item.name, item.rarity));
    if let Some(slot) = item.equip_slot {
        ui.label(format!("Equip slot: {:?}", slot));
    }
    if item.weapon.is_some() {
        let preview_slot = match item.equip_slot {
            Some(crate::items::EquipSlot::OffHand) => WeaponSlot::Secondary,
            _ => WeaponSlot::Primary,
        };
        draw_weapon_preview_lines(ui, item, preview_slot, player_stats);
    }
    if item.weapon.is_none() && item.consumable.is_none() {
        ui.label(format!(
            "Mods: V {} | S {} | A {} | M {} | E {}",
            item.mods.vigor,
            item.mods.strength,
            item.mods.agility,
            item.mods.magic,
            item.mods.endurance,
        ));
    }
}

fn draw_weapon_preview_lines(ui: &mut egui::Ui, item: &Item, slot: WeaponSlot, player_stats: &PlayerStats) {
    if let Some(preview) = weapon_preview_stats(Some(item), slot, player_stats) {
        ui.label(format!("Damage: {:.1}", preview.damage));
        ui.label(format!("Swing: {:.2}s", preview.swing_seconds));
        ui.label(format!("Reach: {:.2}m", preview.reach));
    }
}

fn draw_trader_item_compare(ui: &mut egui::Ui, item: &Item, equip: &Equipment, player_stats: &PlayerStats) {
    let Some((equipped, slot)) = equipped_item_for_comparison(item, equip) else { return; };

    if item.weapon.is_some() {
        let Some(candidate) = weapon_preview_stats(Some(item), slot, player_stats) else { return; };
        let Some(current) = weapon_preview_stats(Some(equipped), slot, player_stats) else { return; };
        ui.small(format!(
            "vs equipped: {} dmg  {} swing  {} reach",
            format_signed(candidate.damage - current.damage, 1, false),
            format_signed(candidate.swing_seconds - current.swing_seconds, 2, true),
            format_signed(candidate.reach - current.reach, 2, false),
        ));
    } else {
        ui.small(format!(
            "vs equipped: V {}  S {}  A {}  M {}  E {}",
            format_signed_i32(item.mods.vigor - equipped.mods.vigor),
            format_signed_i32(item.mods.strength - equipped.mods.strength),
            format_signed_i32(item.mods.agility - equipped.mods.agility),
            format_signed_i32(item.mods.magic - equipped.mods.magic),
            format_signed_i32(item.mods.endurance - equipped.mods.endurance),
        ));
    }
}

fn equipped_item_for_comparison<'a>(item: &Item, equip: &'a Equipment) -> Option<(&'a Item, WeaponSlot)> {
    use crate::items::EquipSlot;

    match item.equip_slot {
        Some(EquipSlot::Hat) => equip.hat.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::Cape) => equip.cape.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::Necklace) => equip.necklace.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::Shirt) => equip.shirt.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::Gloves) => equip.gloves.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::Pants) => equip.pants.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::Shoes) => equip.shoes.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::Bag) => equip.bag.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::Watch) => equip.watch.as_ref().map(|it| (it, WeaponSlot::Primary)),
        Some(EquipSlot::MainHand) => equip.mainhand.as_ref().map(|it| (it, WeaponSlot::Primary)).or_else(|| equip.twohand.as_ref().map(|it| (it, WeaponSlot::Primary))),
        Some(EquipSlot::OffHand) => equip.offhand.as_ref().map(|it| (it, WeaponSlot::Secondary)),
        Some(EquipSlot::TwoHanded) => equip.twohand.as_ref().map(|it| (it, WeaponSlot::Primary)),
        None => None,
    }
}

fn chest_tier_name(tier: ChestTier) -> &'static str {
    match tier {
        ChestTier::Common => "Common",
        ChestTier::Rare => "Rare",
        ChestTier::Epic => "Epic",
        ChestTier::Royal => "Royal",
    }
}

fn format_signed(value: f32, precision: usize, invert_good: bool) -> String {
    let adjusted = if invert_good { -value } else { value };
    let sign = if adjusted >= 0.0 { '+' } else { '-' };
    format!("{}{:.prec$}", sign, adjusted.abs(), prec = precision)
}

fn format_signed_i32(value: i32) -> String {
    if value >= 0 { format!("+{value}") } else { value.to_string() }
}

fn refresh_merchant_inventory(merchant: &mut MerchantStore, seed: u64, state: &MerchantState) {
    let mut rng = StdRng::seed_from_u64(seed);
    let used_refreshes = 3_u8.saturating_sub(state.refreshes_remaining);
    let fill_chance = (0.68 + used_refreshes as f64 * 0.07).clamp(0.68, 0.9);
    for slot in &mut merchant.slots {
        if rng.random_bool(fill_chance) {
            *slot = Some(roll_merchant_stock_item(&mut rng, used_refreshes));
        } else {
            *slot = None;
        }
    }
}

fn roll_merchant_stock_item(rng: &mut StdRng, used_refreshes: u8) -> Item {
    let tries = 2 + used_refreshes as usize;
    let mut best = roll_item(rng);
    for _ in 1..tries {
        let candidate = roll_item(rng);
        if merchant_item_score(&candidate) > merchant_item_score(&best) {
            best = candidate;
        }
    }
    best
}

fn merchant_item_score(item: &Item) -> i32 {
    let rarity_score = match item.rarity {
        crate::items::Rarity::Common => 0,
        crate::items::Rarity::Uncommon => 1,
        crate::items::Rarity::Rare => 2,
        crate::items::Rarity::UltraRare => 3,
        crate::items::Rarity::Legendary => 5,
        crate::items::Rarity::Unique => 8,
    };
    rarity_score * 100 + item.base_value()
}

fn paint_coin_icon(ui: &mut egui::Ui, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size * 1.6, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let c1 = egui::pos2(rect.left() + size * 0.42, rect.center().y);
    let c2 = egui::pos2(rect.left() + size * 0.82, rect.center().y - 1.0);
    let c3 = egui::pos2(rect.left() + size * 1.16, rect.center().y + 1.0);
    for center in [c1, c2, c3] {
        painter.circle_filled(center, size * 0.28, egui::Color32::from_rgb(245, 201, 78));
        painter.circle_stroke(center, size * 0.28, egui::Stroke::new(1.4, egui::Color32::from_rgb(128, 86, 12)));
    }
}

pub fn gold_hud_ui(
    mut ctxs: EguiContexts,
    wallet: Res<PlayerWallet>,
) {
    let Ok(ctx) = ctxs.ctx_mut() else { return; };

    egui::Area::new(egui::Id::new("hud.gold"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-18.0, 18.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(22, 18, 8, 210))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(205, 168, 68)))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        paint_coin_icon(ui, 18.0);
                        ui.label(
                            egui::RichText::new(format!("{}", wallet.gold.max(0)))
                                .strong()
                                .size(18.0)
                                .color(egui::Color32::from_rgb(250, 229, 150)),
                        );
                    });
                });
        });
}

pub fn compass_hud_ui(
    mut ctxs: EguiContexts,
    flags: Res<UIFlags>,
    player_q: Query<(&GlobalTransform, &LookAngles), With<Player>>,
    chests: Query<(&Chest, &GlobalTransform)>,
) {
    if flags.pause_menu_open || flags.inventory_open || flags.spell_menu_open {
        return;
    }

    let Ok(ctx) = ctxs.ctx_mut() else { return; };
    let Ok((player_tf, look)) = player_q.single() else { return; };
    let player_pos = player_tf.translation();

    let mut markers: Vec<(f32, f32, &'static str)> = chests
        .iter()
        .filter_map(|(chest, tf)| {
            let highlight = chest.gold >= 60 || matches!(chest.tier, ChestTier::Epic | ChestTier::Royal);
            if !highlight || chest.gold <= 0 {
                return None;
            }
            let delta = tf.translation() - player_pos;
            let planar = Vec2::new(delta.x, delta.z);
            let dist = planar.length();
            if dist < 0.1 {
                return None;
            }
            let angle = planar.y.atan2(planar.x);
            let relative = wrap_angle(angle - look.yaw);
            Some((relative, dist, chest_tier_name(chest.tier)))
        })
        .collect();

    markers.sort_by(|a, b| a.1.total_cmp(&b.1));
    markers.truncate(4);

    if markers.is_empty() {
        return;
    }

    egui::Area::new(egui::Id::new("hud.compass"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 16.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(12, 14, 18, 180))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(99, 112, 128)))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_min_width(280.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("TREASURE COMPASS").strong().size(12.0));
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(260.0, 38.0), egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        let center_x = rect.center().x;
                        painter.line_segment(
                            [egui::pos2(rect.left(), rect.center().y), egui::pos2(rect.right(), rect.center().y)],
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(66, 74, 84)),
                        );
                        painter.text(egui::pos2(center_x, rect.top()), egui::Align2::CENTER_TOP, "N", egui::FontId::proportional(12.0), egui::Color32::from_rgb(220, 228, 236));

                        for (relative, dist, label) in markers.iter().copied() {
                            let offset = (relative / std::f32::consts::PI).clamp(-1.0, 1.0) * 112.0;
                            let x = center_x + offset;
                            let color = if label == "Royal" {
                                egui::Color32::from_rgb(246, 214, 124)
                            } else {
                                egui::Color32::from_rgb(170, 204, 255)
                            };
                            painter.circle_filled(egui::pos2(x, rect.center().y), 4.0, color);
                            painter.text(
                                egui::pos2(x, rect.bottom()),
                                egui::Align2::CENTER_BOTTOM,
                                format!("{} {:.0}m", label, dist),
                                egui::FontId::proportional(11.0),
                                color,
                            );
                        }
                    });
                });
        });
}

fn wrap_angle(angle: f32) -> f32 {
    let mut wrapped = angle;
    while wrapped > std::f32::consts::PI {
        wrapped -= std::f32::consts::TAU;
    }
    while wrapped < -std::f32::consts::PI {
        wrapped += std::f32::consts::TAU;
    }
    wrapped
}