use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use crate::{
    game::{ AppState, UIFlags, OpenChest, ActiveSpell, StartMenuState, StartMenuTab, Spell, Spellbook, Stash},
    stats::{PlayerStats, Stats},
    inventory::{BagGrid, Equipment, Chest},
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
    mut bag: ResMut<BagGrid>,      // player starting bag
    mut tabs: ResMut<StartMenuState>, // which tab is active
) {
    let ctx = ctxs.ctx_mut();

    // Top-level Start screen window
    egui::Window::new("Start Screen")
        .id(egui::Id::new("window.start_screen"))
        .default_size(egui::vec2(700.0, 520.0))
        .collapsible(false)
        .show(ctx, |ui| {

            // --- Tabs header -------------------------------------------------
            ui.horizontal_wrapped(|ui| {
                let main_selected  = tabs.active == StartMenuTab::Main;
                let stash_selected = tabs.active == StartMenuTab::Stash;

                if ui.selectable_label(main_selected,  "Main Menu").clicked() {
                    tabs.active = StartMenuTab::Main;
                }
                if ui.selectable_label(stash_selected, "Stash").clicked() {
                    tabs.active = StartMenuTab::Stash;
                }
            });

            ui.separator();

            // --- Tab content -------------------------------------------------
            match tabs.active {
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
                    // === NEW: SCROLLABLE BAG + STASH PANES ===

                    ui.heading("Inventory Management");
                    ui.label("Click an item in Bag to move it to Stash; click an item in Stash to move it back to Bag.");
                    ui.separator();

                    // Two columns: BAG (left) and STASH (right)
                    ui.columns(2, |cols| {
                        // -------------------- BAG (scrollable) --------------------
                        cols[0].group(|ui| {
                            ui.set_min_size(egui::vec2(320.0, 380.0));
                            ui.heading(format!("Bag ({}x{})", bag.w, bag.h));
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_source(ui.id().with("scroll_bag"))
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    // Show Bag in a simple list—one line per top-left item cell
                                    // (Your BagGrid currently stores items only at their top-left cell.)
                                    for (i, cell) in bag.cells.iter_mut().enumerate() {
                                        let label = if let Some(it) = cell {
                                            format!("{} ({:?})", it.name, it.rarity)
                                        } else {
                                            "• empty •".to_owned()
                                        };
                                        if ui.button(label).clicked() {
                                            if let Some(it) = cell.take() {
                                                if let Some(slot) = stash.slots.iter_mut().find(|s| s.is_none()) {
                                                    *slot = Some(it);
                                                } else {
                                                    // stash full -> put it back
                                                    *cell = Some(it);
                                                }
                                            }
                                        }
                                    }
                                });
                        });

                        // ------------------- STASH (scrollable) -------------------
                        cols[1].group(|ui| {
                            ui.set_min_size(egui::vec2(320.0, 380.0));
                            ui.heading("Stash (100 slots)");
                            ui.separator();

                            egui::ScrollArea::vertical()
                                .id_source(ui.id().with("scroll_stash"))
                                .auto_shrink([false; 2])
                                .show(ui, |ui| {
                                    for slot in stash.slots.iter_mut() {
                                        let label = if let Some(it) = slot {
                                            format!("{} ({:?})", it.name, it.rarity)
                                        } else {
                                            "• empty •".to_owned()
                                        };
                                        if ui.button(label).clicked() {
                                            if let Some(it) = slot.take() {
                                                if !bag.try_add(it.clone()) {
                                                    // bag full -> put back
                                                    *slot = Some(it.clone());
                                                }
                                            }
                                        }
                                    }
                                });
                        });
                    });
                }
            }
        });
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
    let ctx = ctxs.ctx_mut();

    if flags.inventory_open {
        egui::Window::new("Inventory")
            .id(egui::Id::new("window.inventory")) // Window can take an explicit Id
            .default_size(egui::vec2(360.0, 460.0))
            .show(ctx, |ui| {
                let BagGrid { w, h, .. } = *bag;

                ui.push_id("header", |ui| {
                    ui.label(format!("Bag {}x{}", w, h));
                });

                ui.separator();

                // Stable grid id is fine; we scope each cell to make its button identity unique
                egui::Grid::new(egui::Id::new("grid.bag"))
                    .spacing(egui::Vec2::splat(6.0))
                    .show(ui, |ui| {
                        for y in 0..h {
                            for x in 0..w {
                                // Scope for each cell
                                ui.push_id(("bag_cell", x, y), |ui| {
                                    let txt = if let Some(it) = &bag.cells[(y as usize)*(w as usize)+(x as usize)] {
                                        format!("{} ({:?})", it.name, it.rarity)
                                    } else {
                                        "•".into()
                                    };

                                    if ui.button(txt).clicked() {
                                        sel.selected_bag_cell = Some((x, y));
                                    }
                                });
                            }
                            ui.end_row();
                        }
                    });

                ui.separator();

                ui.push_id("equipment", |ui| {
                    ui.heading("Equipment");

                    // Equip selected button scoped for unique id
                    let mut try_equip_selected = false;
                    if sel.selected_bag_cell.is_some() {
                        ui.push_id("btn_equip_selected", |ui| {
                            if ui.button("Equip selected").clicked() {
                                try_equip_selected = true;
                            }
                        });
                    }

                    if try_equip_selected {
                        if let Some((x,y)) = sel.selected_bag_cell.take() {
                            if let Some(item) = bag.remove_at(x,y) {
                                if let Some(swapped) = equip.equip(item, &mut bag) {
                                    // Try putting swap back to bag; if no space, drop it (TODO)
                                    let _ = bag.try_add(swapped);
                                }
                                // recompute stat bonuses
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

                ui.separator();

                ui.push_id("stats_section", |ui| {
                    ui.heading("Stats");
                    let total = Stats {
                        vigor:     ps.base.vigor + ps.bonus.vigor,
                        strength:  ps.base.strength + ps.bonus.strength,
                        agility:   ps.base.agility + ps.bonus.agility,
                        magic:     ps.base.magic + ps.bonus.magic,
                        endurance: ps.base.endurance + ps.bonus.endurance,
                    };

                    // Labels inside scoped ids
                    ui.push_id("lbl_vigor", |ui| { ui.label(format!("Vigor: {} (+{})",     total.vigor,     ps.bonus.vigor)); });
                    ui.push_id("lbl_strength", |ui| { ui.label(format!("Strength: {} (+{})",  total.strength,  ps.bonus.strength)); });
                    ui.push_id("lbl_agility", |ui| { ui.label(format!("Agility: {} (+{})",   total.agility,   ps.bonus.agility)); });
                    ui.push_id("lbl_magic", |ui| { ui.label(format!("Magic: {} (+{})",     total.magic,     ps.bonus.magic)); });
                    ui.push_id("lbl_endurance", |ui| { ui.label(format!("Endurance: {} (+{})", total.endurance, ps.bonus.endurance)); });
                });
            });
    }

    if let Some(target) = open.0 {
        if let Ok(mut chest) = chests.get_mut(target) {
            let chest_win_id = egui::Id::new(("window.chest", target));
            egui::Window::new("Chest")
                .id(chest_win_id)
                .default_size(egui::vec2(280.0, 360.0))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_source(ui.id().with("scroll_items"))
                        .show(ui, |ui| {
                            for (i, it) in chest.items.iter().enumerate() {
                                let col = it.rarity.color();
                                let s = col.to_srgba();
                                let color = egui::Color32::from_rgba_unmultiplied(
                                    (s.red*255.0) as u8, (s.green*255.0) as u8, (s.blue*255.0) as u8, 255
                                );

                                ui.push_id(("btn_take_item", i), |ui| {
                                    let label = format!("{} ({:?})", it.name, it.rarity);
                                    if ui.add(egui::Button::new(label).fill(color)).clicked() {
                                        sel.selected_chest_idx = Some(i);
                                    }
                                });
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
    mut exit: EventWriter<AppExit>,
    mut next: ResMut<NextState<AppState>>,
) {
    if !flags.pause_menu_open { return; }

    let ctx = ctxs.ctx_mut();
    egui::Window::new("Pause")
        .id(egui::Id::new("window.pause"))
        .collapsible(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                if ui.button("Return to Menu").clicked() {
                    flags.pause_menu_open = false;
                    next.set(AppState::Menu);
                }
                if ui.button("Settings").clicked() {
                    // TODO: add settings later
                }
                if ui.button("Exit Game").clicked() {
                    exit.send(AppExit::Success);
                }
            });
        });
}

pub fn spell_menu_ui(
    mut ctxs: EguiContexts,
    flags: Res<UIFlags>,
    mut active: ResMut<ActiveSpell>,
    book: Res<Spellbook>,
) {
    if !flags.spell_menu_open { return; }

    let ctx = ctxs.ctx_mut();
    egui::Window::new("Spells")
        .id(egui::Id::new("window.spells"))
        .default_size(egui::vec2(260.0, 220.0))
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