use egui::Context;
use std::collections::HashMap;
use stealthterm_config::i18n::t;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RoleType {
    Backend,
    Frontend,
    QA,
    ProductManager,
    Documentation,
    Security,
}

impl RoleType {
    pub fn icon(&self) -> &str {
        match self {
            RoleType::Backend => "🔧",
            RoleType::Frontend => "🎨",
            RoleType::QA => "🧪",
            RoleType::ProductManager => "📋",
            RoleType::Documentation => "📝",
            RoleType::Security => "🔒",
        }
    }

    pub fn name(&self) -> String {
        match self {
            RoleType::Backend => t("role.backend").to_string(),
            RoleType::Frontend => t("role.frontend").to_string(),
            RoleType::QA => t("role.qa").to_string(),
            RoleType::ProductManager => t("role.product_manager").to_string(),
            RoleType::Documentation => t("role.documentation").to_string(),
            RoleType::Security => t("role.security").to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoleStatus {
    Working,
    WaitingInput,
    Stopped,
    NotStarted,
}

impl RoleStatus {
    pub fn icon(&self) -> &str {
        match self {
            RoleStatus::Working => "🟢",
            RoleStatus::WaitingInput => "🟡",
            RoleStatus::Stopped => "🔴",
            RoleStatus::NotStarted => "⚪",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RoleInstance {
    pub role_type: RoleType,
    pub status: RoleStatus,
    pub current_task: String,
    pub progress: f32,
    pub tab_id: Option<String>,
}

pub struct RoleDashboard {
    pub visible: bool,
    roles: HashMap<RoleType, RoleInstance>,
}

impl RoleDashboard {
    pub fn new() -> Self {
        let mut roles = HashMap::new();

        for role_type in [
            RoleType::Backend,
            RoleType::Frontend,
            RoleType::QA,
            RoleType::ProductManager,
            RoleType::Documentation,
            RoleType::Security,
        ] {
            roles.insert(role_type.clone(), RoleInstance {
                role_type: role_type.clone(),
                status: RoleStatus::NotStarted,
                current_task: t("role.not_started").to_string(),
                progress: 0.0,
                tab_id: None,
            });
        }

        Self {
            visible: false,
            roles,
        }
    }

    pub fn show(&mut self, ctx: &Context) -> Option<RoleType> {
        let mut selected_role = None;

        if !self.visible {
            return None;
        }

        egui::Window::new(t("role.dashboard_title"))
            .open(&mut self.visible)
            .default_width(600.0)
            .show(ctx, |ui| {
                ui.heading(t("role.list_heading"));
                ui.separator();

                for role_type in [
                    RoleType::Backend,
                    RoleType::Frontend,
                    RoleType::QA,
                    RoleType::ProductManager,
                    RoleType::Documentation,
                    RoleType::Security,
                ] {
                    if let Some(role) = self.roles.get(&role_type) {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} {}", role.status.icon(), role.role_type.icon()));
                            ui.label(role.role_type.name());
                            ui.separator();
                            ui.label(&role.current_task);

                            if ui.button(t("role.view")).clicked() {
                                selected_role = Some(role_type.clone());
                            }
                        });

                        ui.add(egui::ProgressBar::new(role.progress).show_percentage());
                        ui.add_space(8.0);
                    }
                }
            });

        selected_role
    }

    pub fn update_role_status(&mut self, role_type: RoleType, status: RoleStatus, task: String) {
        if let Some(role) = self.roles.get_mut(&role_type) {
            role.status = status;
            role.current_task = task;
        }
    }
}

impl Default for RoleDashboard {
    fn default() -> Self {
        Self::new()
    }
}
