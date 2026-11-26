//! Popover for creating or editing an interface: chain sides plus atom inclusion criteria.

use super::super::widgets::{button_popover, close_popover, popover_header};
use crate::dataset::ViewerDataset;
use crate::loader::{GROUP_A_NAME, GROUP_B_NAME};
use crate::style::InterfaceSidePalette;
use egui::{Color32, RichText, ScrollArea, Ui};
use smorgasprot_core::{ChainName, InterfaceCriterion, InterfacePartition, InterfaceSide};

/// Rows of the chain-side list shown before it scrolls.
const VISIBLE_CHAIN_ROWS: f32 = 8.0;

/// Which interface the builder writes to on apply.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BuilderTarget {
    New,
    Edit(usize),
}

/// In-progress interface definition; committed to `ViewerDataset` only on apply.
#[derive(Clone)]
pub struct InterfaceDraft {
    target: BuilderTarget,
    name: String,
    /// 1:1 with `ViewerDataset::chains`.
    chain_sides: Vec<InterfaceSide>,
    criteria: Vec<InterfaceCriterion>,
    error: Option<String>,
}

impl InterfaceDraft {
    fn new(ds: &ViewerDataset, target: BuilderTarget) -> Self {
        match target.edit_partition(ds) {
            Some(p) => Self {
                target,
                name: p.name.clone(),
                chain_sides: ds.chains.iter().map(|c| p.chain_side(c.chain_id.as_str())).collect(),
                criteria: p.criteria.clone(),
                error: None,
            },
            // Pre-pairs the first two chains, the common two-chain case.
            None => Self {
                target: BuilderTarget::New,
                name: format!("Interface {}", ds.interfaces.len() + 1),
                chain_sides: (0..ds.chains.len())
                    .map(|i| match i {
                        0 => InterfaceSide::SideA,
                        1 => InterfaceSide::SideB,
                        _ => InterfaceSide::None,
                    })
                    .collect(),
                criteria: InterfaceCriterion::defaults(),
                error: None,
            },
        }
    }

    fn chains_on(&self, ds: &ViewerDataset, side: InterfaceSide) -> Vec<ChainName> {
        ds.chains
            .iter()
            .zip(&self.chain_sides)
            .filter(|(_, s)| **s == side)
            .map(|(c, _)| c.chain_id.clone())
            .collect()
    }

    /// Reason the draft can't be applied yet, if any.
    fn blocker(&self) -> Option<&'static str> {
        if !self.chain_sides.contains(&InterfaceSide::SideA) {
            Some("Assign at least one chain to Group A.")
        } else if !self.chain_sides.contains(&InterfaceSide::SideB) {
            Some("Assign at least one chain to Group B.")
        } else if self.criteria.is_empty() {
            Some("Add at least one inclusion criterion.")
        } else {
            None
        }
    }

    fn apply(&self, ds: &mut ViewerDataset) -> Result<(), String> {
        let (group_a_name, group_b_name) = match self.target.edit_partition(ds) {
            Some(p) => (p.group_a_name.clone(), p.group_b_name.clone()),
            None => (GROUP_A_NAME.to_owned(), GROUP_B_NAME.to_owned()),
        };
        let name = match self.name.trim() {
            "" => format!("Interface {}", ds.interfaces.len() + 1),
            trimmed => trimmed.to_owned(),
        };
        let partition = InterfacePartition::new(
            name,
            group_a_name,
            self.chains_on(ds, InterfaceSide::SideA),
            group_b_name,
            self.chains_on(ds, InterfaceSide::SideB),
        )
        .with_criteria(self.criteria.clone());
        match self.target {
            BuilderTarget::New => ds.add_interface(partition).map(|_| ()),
            BuilderTarget::Edit(idx) => ds.replace_interface(idx, partition),
        }
    }
}

impl BuilderTarget {
    fn edit_partition(self, ds: &ViewerDataset) -> Option<&InterfacePartition> {
        match self {
            Self::New => None,
            Self::Edit(idx) => ds.interfaces.get(idx).map(|i| &i.partition),
        }
    }
}

/// Draws `button` as the toggle for a builder popover writing to `target`.
pub fn builder_popover(
    ui: &mut Ui,
    button: &egui::Response,
    ds: &mut ViewerDataset,
    target: BuilderTarget,
    draft: &mut Option<InterfaceDraft>,
    sides: &InterfaceSidePalette,
    danger_color: Color32,
) {
    let id_salt = match target {
        BuilderTarget::New => "interface_builder_new",
        BuilderTarget::Edit(_) => "interface_builder_edit",
    };
    let opened = button_popover(ui, id_salt, button, |ui| {
        let stale = draft
            .as_ref()
            .is_none_or(|d| d.target != target || d.chain_sides.len() != ds.chains.len());
        if stale {
            *draft = Some(InterfaceDraft::new(ds, target));
        }
        if let Some(d) = draft.as_mut()
            && builder_contents(ui, ds, d, sides, danger_color)
        {
            *draft = None;
            close_popover(ui.ctx());
        }
    });
    if opened {
        *draft = Some(InterfaceDraft::new(ds, target));
    }
}

/// Returns `true` once the draft has been successfully applied or cancelled.
fn builder_contents(
    ui: &mut Ui,
    ds: &mut ViewerDataset,
    draft: &mut InterfaceDraft,
    sides: &InterfaceSidePalette,
    danger_color: Color32,
) -> bool {
    let title = match draft.target {
        BuilderTarget::New => "New interface",
        BuilderTarget::Edit(_) => "Edit interface",
    };
    popover_header(ui, true, title, false, || {});

    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(f32::INFINITY));
    });

    ui.add_space(4.0);
    ui.label(RichText::new("Chain groups").strong());
    chain_side_list(ui, ds, draft, sides);

    ui.add_space(4.0);
    ui.label(RichText::new("Inclusion criteria").strong());
    ui.weak("An atom on either side is interfacial if it meets any criterion.");
    criteria_list(ui, &mut draft.criteria);

    if let Some(err) = &draft.error {
        ui.colored_label(danger_color, err);
    }

    ui.separator();
    let blocker = draft.blocker();
    let mut done = false;
    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            done = true;
        }
        let apply_label = match draft.target {
            BuilderTarget::New => "Create",
            BuilderTarget::Edit(_) => "Apply",
        };
        let resp = ui.add_enabled(blocker.is_none(), egui::Button::new(RichText::new(apply_label).strong()));
        let resp = match blocker {
            Some(reason) => resp.on_disabled_hover_text(reason),
            None => resp,
        };
        if resp.clicked() {
            match draft.apply(ds) {
                Ok(()) => done = true,
                Err(e) => draft.error = Some(e),
            }
        }
    });
    done
}

fn chain_side_list(ui: &mut Ui, ds: &ViewerDataset, draft: &mut InterfaceDraft, sides: &InterfaceSidePalette) {
    let row_h = ui.spacing().interact_size.y;
    let max_h = VISIBLE_CHAIN_ROWS * (row_h + ui.spacing().item_spacing.y);
    ScrollArea::vertical()
        .id_salt("interface_builder_chains")
        .max_height(max_h)
        .show(ui, |ui| {
            egui::Grid::new("interface_builder_chain_grid")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (chain, side) in ds.chains.iter().zip(draft.chain_sides.iter_mut()) {
                        ui.label(format!("Chain {}", chain.chain_id))
                            .on_hover_text(format!("{} residues, {} atoms", chain.residue_count, chain.atom_count));
                        ui.horizontal(|ui| {
                            for (value, label) in
                                [(InterfaceSide::SideA, "A"), (InterfaceSide::SideB, "B"), (InterfaceSide::None, "—")]
                            {
                                let text = match value {
                                    InterfaceSide::None => RichText::new(label),
                                    _ => RichText::new(label).strong().color(sides.side_light(value)),
                                };
                                ui.selectable_value(side, value, text);
                            }
                        });
                        ui.end_row();
                    }
                });
        });
}

fn criteria_list(ui: &mut Ui, criteria: &mut Vec<InterfaceCriterion>) {
    let mut remove = None;
    for (i, criterion) in criteria.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("interface_builder_criterion", i))
                .selected_text(criterion_label(*criterion))
                .show_ui(ui, |ui| {
                    for option in [InterfaceCriterion::buried_sasa(), InterfaceCriterion::proximity()] {
                        let selected = std::mem::discriminant(criterion) == std::mem::discriminant(&option);
                        if ui.selectable_label(selected, criterion_label(option)).clicked() && !selected {
                            *criterion = option;
                        }
                    }
                });
            match criterion {
                InterfaceCriterion::BuriedSasa { min_a2 } => {
                    ui.label(">");
                    ui.add(egui::DragValue::new(min_a2).range(0.0..=50.0).speed(0.05).suffix(" Å²"));
                }
                InterfaceCriterion::Proximity { cutoff_a } => {
                    ui.label("≤");
                    ui.add(egui::DragValue::new(cutoff_a).range(0.5..=12.0).speed(0.05).suffix(" Å"));
                }
            }
            if ui.small_button("×").on_hover_text("Remove criterion").clicked() {
                remove = Some(i);
            }
        })
        .response
        .on_hover_text(criterion_description(*criterion));
    }
    if let Some(i) = remove {
        criteria.remove(i);
    }
    ui.menu_button("+ Add criterion", |ui| {
        for option in [InterfaceCriterion::buried_sasa(), InterfaceCriterion::proximity()] {
            if ui.button(criterion_label(option)).on_hover_text(criterion_description(option)).clicked() {
                criteria.push(option);
                ui.close_menu();
            }
        }
    });
}

fn criterion_label(c: InterfaceCriterion) -> &'static str {
    match c {
        InterfaceCriterion::BuriedSasa { .. } => "Buried area (ΔSASA)",
        InterfaceCriterion::Proximity { .. } => "Cross-side distance",
    }
}

fn criterion_description(c: InterfaceCriterion) -> &'static str {
    match c {
        InterfaceCriterion::BuriedSasa { .. } => {
            "Solvent-accessible surface the atom loses when the two groups bind. The standard interface definition."
        }
        InterfaceCriterion::Proximity { .. } => {
            "Center-to-center distance from the atom to the nearest atom of the opposite group."
        }
    }
}
