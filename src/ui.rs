use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use crate::{
    game::AppState,
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
) {
    let ctx = ctxs.ctx_mut();

    // CentralPanel has a stable internal id; we create our own id namespace using push_id.
    egui::CentralPanel::default().show(ctx, |ui| {
        // Root scope for the menu
        ui.push_id("menu_root", |ui| {
            ui.heading("Customize Character");
            ui.separator();
            ui.label("Distribute +5 free points (not persisted across sessions yet).");

            // Stable container id for the stat editor
            ui.push_id("stats_editor", |ui| {
                let mut b = ps.base;

                // Create a stable order listing; attach scoped ids per row and per +/- button
                for (row_ix, (label, val)) in [
                    ("Vigor",     &mut b.vigor),
                    ("Strength",  &mut b.strength),
                    ("Agility",   &mut b.agility),
                    ("Magic",     &mut b.magic),
                    ("Endurance", &mut b.endurance),
                ]
                .into_iter()
                .enumerate()
                {
                    ui.push_id(format!("row_{row_ix}"), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(label);

                            // Minus button with a stable id by scoping:
                            ui.push_id("btn_minus", |ui| {
                                if ui.button("-").clicked() {
                                    *val = (*val - 1).max(1);
                                }
                            });

                            // Value label (labels don't use ids in egui; scoping is enough)
                            ui.push_id("value", |ui| {
                                ui.label(format!("{label}: {}", *val));
                            });

                            // Plus button with a stable id by scoping:
                            ui.push_id("btn_plus", |ui| {
                                if ui.button("+").clicked() {
                                    *val += 1;
                                }
                            });
                        });
                    });
                }

                // "Play" button with a stable id by scoping:
                ui.push_id("btn_play", |ui| {
                    if ui.button("Play").clicked() {
                        ps.base = b;
                        ps.recompute_limbs();
                        next.set(AppState::InGame);
                    }
                });
            });
        });
    });
}

/// Inventory & equipment UI, and chest UI when open
pub fn inventory_ui(
    mut ctxs: EguiContexts,
    mut sel: ResMut<UISelection>,
    mut bag: ResMut<BagGrid>,
    mut equip: ResMut<Equipment>,
    mut ps: ResMut<PlayerStats>,
    mut chests: Query<&mut Chest>,
) {
    let ctx = ctxs.ctx_mut();

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

    // Chest windows. Give each window and each item button a unique scope.
    for (chest_ix, mut chest) in chests.iter_mut().enumerate() {
        let chest_win_id = egui::Id::new(("window.chest", chest_ix));

        egui::Window::new("Chest")
            .id(chest_win_id)
            .default_size(egui::vec2(280.0, 360.0))
            .show(ctx, |ui| {
                // Scroll area with stable id is supported:
                egui::ScrollArea::vertical()
                    .id_source(ui.id().with("scroll_items"))
                    .show(ui, |ui| {
                        for (i, it) in chest.items.iter().enumerate() {
                            let col = it.rarity.color();
                            let s = col.to_srgba();
                            let color = egui::Color32::from_rgba_unmultiplied(
                                (s.red*255.0) as u8, (s.green*255.0) as u8, (s.blue*255.0) as u8, 255
                            );

                            // Scope each item so the button id is unique/stable
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