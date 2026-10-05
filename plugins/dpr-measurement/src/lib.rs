use std::sync::{Mutex, OnceLock};

use std::collections::HashMap;

use ocs_plugin_api::host::{BuiltinPlugin, CommandStep, Handle, HostApi, InteractiveCommand};
use ocs_plugin_api::manifest::{ApiVersion, PluginManifest};
use ocs_plugin_api::ribbon::{CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef};

static MANIFEST: PluginManifest = PluginManifest {
    id: "opencad.dpr.measurement",
    name: "DPR Measurement",
    version: "0.1.0",
    description: "Wall and activity quantity tracking for DPR reporting.",
    api_version: ApiVersion::CURRENT,
    ribbon_order: 80,
    xdata_apps: &["DPRMEAS"],
    command_prefixes: &["DPR_"],
};

#[derive(Clone, Debug, PartialEq)]
struct DprEntry {
    wall_id: String,
    activity: ActivityKind,
    length_m: f64,
    height_m: f64,
    quantity_m2: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum ActivityKind {
    BlockWork,
    InternalPlaster,
    ExternalPlaster,
    TileFlooring,
    Waterproofing,
    Concrete,
}

impl ActivityKind {
    fn from_str(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().replace('_', " ").as_str() {
            "block work" => Some(Self::BlockWork),
            "internal plaster" => Some(Self::InternalPlaster),
            "external plaster" => Some(Self::ExternalPlaster),
            "tile flooring" => Some(Self::TileFlooring),
            "waterproofing" => Some(Self::Waterproofing),
            "concrete" => Some(Self::Concrete),
            _ => None,
        }
    }

    fn as_label(&self) -> &'static str {
        match self {
            Self::BlockWork => "Block Work",
            Self::InternalPlaster => "Internal Plaster",
            Self::ExternalPlaster => "External Plaster",
            Self::TileFlooring => "Tile Flooring",
            Self::Waterproofing => "Waterproofing",
            Self::Concrete => "Concrete",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DrawingUnit {
    Millimeters,
    Centimeters,
    Meters,
    Inches,
    Feet,
}

impl DrawingUnit {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "MM" | "MILLIMETERS" | "MILLIMETRES" => Some(Self::Millimeters),
            "CM" | "CENTIMETERS" | "CENTIMETRES" => Some(Self::Centimeters),
            "M" | "METER" | "METERS" | "METRE" | "METRES" => Some(Self::Meters),
            "IN" | "INCH" | "INCHES" => Some(Self::Inches),
            "FT" | "FOOT" | "FEET" => Some(Self::Feet),
            _ => None,
        }
    }

    fn meters_per_unit(self) -> f64 {
        match self {
            Self::Millimeters => 0.001,
            Self::Centimeters => 0.01,
            Self::Meters => 1.0,
            Self::Inches => 0.0254,
            Self::Feet => 0.3048,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Millimeters => "mm",
            Self::Centimeters => "cm",
            Self::Meters => "m",
            Self::Inches => "in",
            Self::Feet => "ft",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DprDailyEntry {
    date: String,
    activity: ActivityKind,
    quantity_m2: f64,
    progress_pct: f64,
    remarks: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DprPlanEntry {
    activity: ActivityKind,
    planned_m2: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct ScheduleItem {
    title: String,
    start_date: String,
    end_date: String,
    progress_pct: f64,
}

#[derive(Clone, Debug)]
struct DprProjectState {
    project: String,
    floor: String,
    wall_height_m: f64,
    wall_thickness_m: f64,
    drawing_unit: DrawingUnit,
    entries: Vec<DprEntry>,
    daily_entries: Vec<DprDailyEntry>,
    planned_entries: Vec<DprPlanEntry>,
    schedule: Vec<ScheduleItem>,
}

impl Default for DprProjectState {
    fn default() -> Self {
        Self {
            project: "IIIT Dharwad Hostel".to_string(),
            floor: "Ground Floor".to_string(),
            wall_height_m: 3.0,
            wall_thickness_m: 0.2,
            drawing_unit: DrawingUnit::Meters,
            entries: Vec::new(),
            daily_entries: Vec::new(),
            planned_entries: Vec::new(),
            schedule: Vec::new(),
        }
    }
}

static DPR_STATE: OnceLock<Mutex<DprProjectState>> = OnceLock::new();

fn with_dpr_state<R>(f: impl FnOnce(&mut DprProjectState) -> R) -> R {
    let mut state = DPR_STATE
        .get_or_init(|| Mutex::new(DprProjectState::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut state)
}

impl DprProjectState {
    fn add_schedule_item(
        &mut self,
        title: &str,
        start_date: &str,
        end_date: &str,
        progress_pct: f64,
    ) {
        self.schedule.retain(|item| item.title != title);
        self.schedule.push(ScheduleItem {
            title: title.to_string(),
            start_date: start_date.to_string(),
            end_date: end_date.to_string(),
            progress_pct: progress_pct.clamp(0.0, 100.0),
        });
    }

    fn schedule_text(&self) -> String {
        if self.schedule.is_empty() {
            return "No project schedule configured yet.".to_string();
        }

        let lines = self
            .schedule
            .iter()
            .map(|item| {
                format!(
                    "{} | {} -> {} | {:.1}%",
                    item.title,
                    item.start_date,
                    item.end_date,
                    item.progress_pct
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!("Project schedule:\n{}", lines)
    }

    fn save_to_file(&self, path: &str) -> Result<(), String> {
        let mut rows = Vec::new();
        rows.push(format!("PROJECT|{}", self.project));
        rows.push(format!("FLOOR|{}", self.floor));

        for entry in &self.entries {
            rows.push(format!(
                "MEAS|{}|{}|{}|{}|{}",
                entry.wall_id,
                entry.activity.as_label(),
                entry.length_m,
                entry.height_m,
                entry.quantity_m2
            ));
        }

        for entry in &self.daily_entries {
            rows.push(format!(
                "DAILY|{}|{}|{}|{}|{}",
                entry.date,
                entry.activity.as_label(),
                entry.quantity_m2,
                entry.progress_pct,
                entry.remarks
            ));
        }

        for plan in &self.planned_entries {
            rows.push(format!(
                "PLAN|{}|{}",
                plan.activity.as_label(),
                plan.planned_m2
            ));
        }

        for item in &self.schedule {
            rows.push(format!(
                "SCHEDULE|{}|{}|{}|{}",
                item.title,
                item.start_date,
                item.end_date,
                item.progress_pct
            ));
        }

        std::fs::write(path, rows.join("\n")).map_err(|err| err.to_string())?;
        Ok(())
    }

    fn load_from_file(path: &str) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
        let mut state = Self::default();

        for line in contents.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split('|').collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0] {
                "PROJECT" => {
                    if parts.len() > 1 {
                        state.project = parts[1].to_string();
                    }
                }
                "FLOOR" => {
                    if parts.len() > 1 {
                        state.floor = parts[1].to_string();
                    }
                }
                "MEAS" => {
                    if parts.len() >= 6 {
                        let wall_id = parts[1];
                        let activity = parts[2];
                        let length_m = parts[3].parse::<f64>().unwrap_or(0.0);
                        let height_m = parts[4].parse::<f64>().unwrap_or(0.0);
                        let quantity_m2 = parts[5].parse::<f64>().unwrap_or(0.0);
                        state.entries.push(DprEntry {
                            wall_id: wall_id.to_string(),
                            activity: ActivityKind::from_str(activity).unwrap_or(ActivityKind::BlockWork),
                            length_m,
                            height_m,
                            quantity_m2,
                        });
                    }
                }
                "DAILY" => {
                    if parts.len() >= 6 {
                        let date = parts[1];
                        let activity = parts[2];
                        let quantity = parts[3].parse::<f64>().unwrap_or(0.0);
                        let progress = parts[4].parse::<f64>().unwrap_or(0.0);
                        let remarks = parts[5..].join("|");
                        state.daily_entries.push(DprDailyEntry {
                            date: date.to_string(),
                            activity: ActivityKind::from_str(activity).unwrap_or(ActivityKind::BlockWork),
                            quantity_m2: quantity,
                            progress_pct: progress,
                            remarks,
                        });
                    }
                }
                "PLAN" => {
                    if parts.len() >= 3 {
                        let activity = parts[1];
                        let planned = parts[2].parse::<f64>().unwrap_or(0.0);
                        state.planned_entries.push(DprPlanEntry {
                            activity: ActivityKind::from_str(activity).unwrap_or(ActivityKind::BlockWork),
                            planned_m2: planned,
                        });
                    }
                }
                "SCHEDULE" => {
                    if parts.len() >= 5 {
                        let title = parts[1];
                        let start = parts[2];
                        let end = parts[3];
                        let progress = parts[4].parse::<f64>().unwrap_or(0.0);
                        state.schedule.push(ScheduleItem {
                            title: title.to_string(),
                            start_date: start.to_string(),
                            end_date: end.to_string(),
                            progress_pct: progress,
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(state)
    }

    fn set_project(&mut self, project: &str) {
        self.project = project.to_string();
    }

    fn set_floor(&mut self, floor: &str) {
        self.floor = floor.to_string();
    }

    fn set_plan(&mut self, activity: &str, planned_m2: f64) {
        let activity = match ActivityKind::from_str(activity) {
            Some(kind) => kind,
            None => ActivityKind::BlockWork,
        };

        self.planned_entries.retain(|entry| entry.activity != activity);
        self.planned_entries.push(DprPlanEntry {
            activity,
            planned_m2: planned_m2.max(0.0),
        });
    }

    fn add_measurement(
        &mut self,
        wall_id: &str,
        activity: &str,
        length_m: f64,
        height_m: f64,
    ) {
        let activity = match ActivityKind::from_str(activity) {
            Some(kind) => kind,
            None => ActivityKind::BlockWork,
        };

        let quantity_m2 = calculate_wall_area(length_m, height_m);
        self.entries.push(DprEntry {
            wall_id: wall_id.to_string(),
            activity,
            length_m,
            height_m,
            quantity_m2,
        });
    }

    fn add_picked_wall(&mut self, length_m: f64, height_m: f64) -> String {
        let wall_id = format!("W-{:02}", self.entries.len() + 1);
        self.add_measurement(&wall_id, "Block_Work", length_m, height_m);
        wall_id
    }

    fn add_daily_entry(
        &mut self,
        date: &str,
        activity: &str,
        quantity_m2: f64,
        progress_pct: f64,
        remarks: &str,
    ) {
        let activity = match ActivityKind::from_str(activity) {
            Some(kind) => kind,
            None => ActivityKind::BlockWork,
        };

        self.daily_entries.push(DprDailyEntry {
            date: date.to_string(),
            activity,
            quantity_m2: quantity_m2.max(0.0),
            progress_pct: progress_pct.clamp(0.0, 100.0),
            remarks: remarks.to_string(),
        });
    }

    fn total_quantity(&self) -> f64 {
        self.entries.iter().map(|entry| entry.quantity_m2).sum()
    }

    fn activity_totals(&self) -> Vec<(String, f64)> {
        let mut map: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
        for entry in &self.entries {
            let label = entry.activity.as_label().to_string();
            let total = map.entry(label).or_insert(0.0);
            *total += entry.quantity_m2;
        }
        map.into_iter().collect()
    }

    fn total_daily_quantity(&self) -> f64 {
        self.daily_entries.iter().map(|entry| entry.quantity_m2).sum()
    }

    fn planned_vs_actual_text(&self) -> String {
        if self.planned_entries.is_empty() && self.entries.is_empty() {
            return "No planned quantities set.".to_string();
        }

        let actual_by_activity = self
            .entries
            .iter()
            .fold(std::collections::BTreeMap::<String, f64>::new(), |mut map, entry| {
                let key = entry.activity.as_label().to_string();
                *map.entry(key).or_insert(0.0) += entry.quantity_m2;
                map
            });

        let rows = self
            .planned_entries
            .iter()
            .map(|plan| {
                let actual = actual_by_activity.get(plan.activity.as_label()).copied().unwrap_or(0.0);
                let pct = if plan.planned_m2 > 0.0 {
                    (actual / plan.planned_m2) * 100.0
                } else {
                    0.0
                };
                let variance = actual - plan.planned_m2;
                format!(
                    "{} | Planned {:.3} m² | Actual {:.3} m² | Completion {:.1}% | Variance {:.3} m²",
                    plan.activity.as_label(),
                    plan.planned_m2,
                    actual,
                    pct,
                    variance
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let total_plan: f64 = self.planned_entries.iter().map(|entry| entry.planned_m2).sum();
        let total_actual: f64 = self.entries.iter().map(|entry| entry.quantity_m2).sum();
        let overall_pct = if total_plan > 0.0 { (total_actual / total_plan) * 100.0 } else { 0.0 };

        format!(
            "Planned vs Actual:\n{}\nOverall completion = {:.1}% (Actual {:.3} m² / Planned {:.3} m²)",
            rows,
            overall_pct,
            total_actual,
            total_plan
        )
    }

    fn daily_table_text(&self) -> String {
        if self.daily_entries.is_empty() {
            return "No daily DPR entries recorded yet.".to_string();
        }

        let lines = self
            .daily_entries
            .iter()
            .map(|entry| {
                format!(
                    "{} | {} | {:.3} m² | {:.1}% | {}",
                    entry.date,
                    entry.activity.as_label(),
                    entry.quantity_m2,
                    entry.progress_pct,
                    if entry.remarks.is_empty() {
                        "-"
                    } else {
                        entry.remarks.as_str()
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "Daily DPR table:\nDate | Activity | Quantity | Progress | Remarks\n{}\nDaily total = {:.3} m²",
            lines,
            self.total_daily_quantity()
        )
    }

    fn dashboard_text(&self) -> String {
        let total_plan: f64 = self.planned_entries.iter().map(|entry| entry.planned_m2).sum();
        let total_actual: f64 = self.total_quantity();
        let overall_pct = if total_plan > 0.0 { (total_actual / total_plan) * 100.0 } else { 0.0 };

        let latest_day = self
            .daily_entries
            .iter()
            .max_by(|a, b| a.date.cmp(&b.date))
            .map(|entry| entry.date.as_str())
            .unwrap_or("No daily log");

        let daily_total = self.total_daily_quantity();
        let activity_summary = self
            .planned_entries
            .iter()
            .map(|plan| {
                let actual = self
                    .entries
                    .iter()
                    .filter(|entry| entry.activity == plan.activity)
                    .map(|entry| entry.quantity_m2)
                    .sum::<f64>();
                let completion = if plan.planned_m2 > 0.0 { (actual / plan.planned_m2) * 100.0 } else { 0.0 };
                format!(
                    "{}: {:.1}% complete | Planned {:.3} m² | Actual {:.3} m²",
                    plan.activity.as_label(),
                    completion,
                    plan.planned_m2,
                    actual
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let schedule_summary = self
            .schedule
            .iter()
            .map(|task| {
                format!(
                    "{}: {} to {} | {:.1}%",
                    task.title,
                    task.start_date,
                    task.end_date,
                    task.progress_pct
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let status = if overall_pct >= 90.0 {
            "Ahead of schedule"
        } else if overall_pct >= 60.0 {
            "On track"
        } else if overall_pct >= 30.0 {
            "Needs attention"
        } else {
            "Behind plan"
        };

        format!(
            "Site Dashboard\nProject: {}\nFloor: {}\nStatus: {}\nOverall completion: {:.1}%\nActual: {:.3} m² | Planned: {:.3} m²\nDaily output: {:.3} m²\nLast log date: {}\n\nActivity progress:\n{}\n\nProject schedule:\n{}",
            self.project,
            self.floor,
            status,
            overall_pct,
            total_actual,
            total_plan,
            daily_total,
            latest_day,
            if activity_summary.is_empty() { "No planned activities configured.".to_string() } else { activity_summary },
            if schedule_summary.is_empty() { "No project schedule configured.".to_string() } else { schedule_summary }
        )
    }

    fn summary_text(&self) -> String {
        if self.entries.is_empty() && self.daily_entries.is_empty() && self.planned_entries.is_empty() {
            return "No DPR entries recorded yet.".to_string();
        }

        let total = self.total_quantity();
        let lines = self
            .entries
            .iter()
            .map(|entry| {
                format!(
                    "{} | {} | L={} m | H={} m | Qty={} m²",
                    entry.wall_id,
                    entry.activity.as_label(),
                    entry.length_m,
                    entry.height_m,
                    entry.quantity_m2
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let activity_lines = self
            .activity_totals()
            .iter()
            .map(|(activity, qty)| format!("{} = {:.3} m²", activity, qty))
            .collect::<Vec<_>>()
            .join("\n");

        let daily_lines = self.daily_table_text();
        let progress_lines = self.planned_vs_actual_text();

        format!(
            "Project: {}\nFloor: {}\nEntries:\n{}\nActivity totals:\n{}\nDaily progress:\n{}\nPlanned progress:\n{}\nTotal = {:.3} m²",
            self.project, self.floor, lines, activity_lines, daily_lines, progress_lines, total
        )
    }

    fn csv_text(&self) -> String {
        let mut rows = Vec::new();
        rows.push("Wall ID,Activity,Length (m),Height (m),Quantity (m2)".to_string());
        for entry in &self.entries {
            rows.push(format!(
                "{},{},{},{},{}",
                entry.wall_id,
                entry.activity.as_label(),
                entry.length_m,
                entry.height_m,
                entry.quantity_m2
            ));
        }

        if !self.planned_entries.is_empty() {
            rows.push("".to_string());
            rows.push("Activity,Planned (m2),Actual (m2),Completion (%),Variance (m2)".to_string());
            let actual_by_activity = self
                .entries
                .iter()
                .fold(std::collections::BTreeMap::<String, f64>::new(), |mut map, entry| {
                    let key = entry.activity.as_label().to_string();
                    *map.entry(key).or_insert(0.0) += entry.quantity_m2;
                    map
                });

            for plan in &self.planned_entries {
                let actual = actual_by_activity.get(plan.activity.as_label()).copied().unwrap_or(0.0);
                let completion = if plan.planned_m2 > 0.0 {
                    (actual / plan.planned_m2) * 100.0
                } else {
                    0.0
                };
                let variance = actual - plan.planned_m2;
                rows.push(format!(
                    "{},{},{},{},{}",
                    plan.activity.as_label(),
                    plan.planned_m2,
                    actual,
                    completion,
                    variance
                ));
            }
        }

        if !self.daily_entries.is_empty() {
            rows.push("".to_string());
            rows.push("Date,Activity,Quantity (m2),Progress (%),Remarks".to_string());
            for entry in &self.daily_entries {
                rows.push(format!(
                    "{},{},{},{},{}",
                    entry.date,
                    entry.activity.as_label(),
                    entry.quantity_m2,
                    entry.progress_pct,
                    entry.remarks
                ));
            }
        }

        rows.join("\n")
    }
}

fn calculate_wall_area(length_m: f64, height_m: f64) -> f64 {
    length_m * height_m
}

fn to_meters(length: f64, unit: DrawingUnit) -> f64 {
    length * unit.meters_per_unit()
}

fn parse_add_command(cmd: &str) -> Option<(&str, &str, f64, f64)> {
    let trimmed = cmd.trim();
    if !trimmed.starts_with("DPR_ADD ") {
        return None;
    }

    let rest = trimmed.trim_start_matches("DPR_ADD ");
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 4 {
        return None;
    }

    let wall_id = parts[0];
    let activity = parts[1];
    let length = parts[2].parse::<f64>().ok()?;
    let height = parts[3].parse::<f64>().ok()?;
    Some((wall_id, activity, length, height))
}

fn parse_set_wall_height_command(cmd: &str) -> Option<f64> {
    let value = cmd.trim().strip_prefix("DPR_SET_WALL_HEIGHT ")?;
    let height = value.trim().parse::<f64>().ok()?;
    (height.is_finite() && height > 0.0).then_some(height)
}

fn parse_set_drawing_unit_command(cmd: &str) -> Option<DrawingUnit> {
    let value = cmd.trim().strip_prefix("DPR_SET_UNITS ")?;
    DrawingUnit::parse(value)
}

fn parse_set_wall_thickness_command(cmd: &str) -> Option<f64> {
    let value = cmd.trim().strip_prefix("DPR_SET_WALL_THICKNESS ")?;
    let thickness_mm = value.trim().parse::<f64>().ok()?;
    (thickness_mm.is_finite() && thickness_mm > 0.0).then_some(thickness_mm / 1000.0)
}

fn parse_set_project_command(cmd: &str) -> Option<&str> {
    let trimmed = cmd.trim();
    if !trimmed.starts_with("DPR_SET_PROJECT ") {
        return None;
    }
    let rest = trimmed.trim_start_matches("DPR_SET_PROJECT ");
    if rest.is_empty() { return None; }
    Some(rest)
}

fn parse_set_floor_command(cmd: &str) -> Option<&str> {
    let trimmed = cmd.trim();
    if !trimmed.starts_with("DPR_SET_FLOOR ") {
        return None;
    }
    let rest = trimmed.trim_start_matches("DPR_SET_FLOOR ");
    if rest.is_empty() { return None; }
    Some(rest)
}

fn parse_daily_entry_command(cmd: &str) -> Option<(&str, &str, f64, f64, String)> {
    let trimmed = cmd.trim();
    if !trimmed.starts_with("DPR_DAY_ADD ") {
        return None;
    }

    let rest = trimmed.trim_start_matches("DPR_DAY_ADD ");
    let mut parts = rest.split_whitespace();
    let date = parts.next()?;
    let activity = parts.next()?;
    let quantity = parts.next()?.parse::<f64>().ok()?;
    let progress = parts.next()?.parse::<f64>().ok()?;
    let remarks = parts.collect::<Vec<_>>().join(" ");
    Some((date, activity, quantity, progress, remarks))
}

fn parse_daily_report_command(cmd: &str) -> bool {
    cmd == "DPR_DAY_REPORT"
}

fn parse_set_plan_command(cmd: &str) -> Option<(&str, f64)> {
    let trimmed = cmd.trim();
    if !trimmed.starts_with("DPR_SET_PLAN ") {
        return None;
    }

    let rest = trimmed.trim_start_matches("DPR_SET_PLAN ");
    let mut parts = rest.split_whitespace();
    let activity = parts.next()?;
    let planned = parts.next()?.parse::<f64>().ok()?;
    Some((activity, planned))
}

fn parse_progress_report_command(cmd: &str) -> bool {
    cmd == "DPR_PROGRESS" || cmd == "DPR_DASHBOARD" || cmd == "DPR_SITE_DASHBOARD"
}

fn parse_add_schedule_command(cmd: &str) -> Option<(&str, &str, &str, f64)> {
    let trimmed = cmd.trim();
    if !trimmed.starts_with("DPR_ADD_TASK ") {
        return None;
    }

    let rest = trimmed.trim_start_matches("DPR_ADD_TASK ");
    let mut parts = rest.split_whitespace();
    let title = parts.next()?;
    let start_date = parts.next()?;
    let end_date = parts.next()?;
    let progress = parts.next()?.parse::<f64>().ok()?;
    Some((title, start_date, end_date, progress))
}

fn parse_save_command(cmd: &str) -> Option<&str> {
    let trimmed = cmd.trim();
    if !trimmed.starts_with("DPR_SAVE ") {
        return None;
    }
    let rest = trimmed.trim_start_matches("DPR_SAVE ");
    if rest.is_empty() { return None; }
    Some(rest)
}

fn parse_load_command(cmd: &str) -> Option<&str> {
    let trimmed = cmd.trim();
    if !trimmed.starts_with("DPR_LOAD ") {
        return None;
    }
    let rest = trimmed.trim_start_matches("DPR_LOAD ");
    if rest.is_empty() { return None; }
    Some(rest)
}

struct DprModule;

impl CadModule for DprModule {
    fn id(&self) -> &'static str {
        "dpr_measurement"
    }

    fn title(&self) -> &'static str {
        "DPR"
    }

    fn ribbon_groups(&self) -> &[RibbonGroup] {
        static GROUPS: OnceLock<Vec<RibbonGroup>> = OnceLock::new();
        GROUPS.get_or_init(|| {
            vec![RibbonGroup {
                title: "Measurement",
                tools: vec![
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_PICK_WALL",
                        label: "Pick Wall",
                        icon: IconKind::Glyph("▣"),
                        event: ModuleEvent::Command("DPR_PICK_WALL".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_SUMMARY",
                        label: "Summary",
                        icon: IconKind::Glyph("Σ"),
                        event: ModuleEvent::Command("DPR_SUMMARY".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_EXPORT",
                        label: "Export CSV",
                        icon: IconKind::Glyph("⇩"),
                        event: ModuleEvent::Command("DPR_EXPORT".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_DAY_ADD",
                        label: "Add Day Log",
                        icon: IconKind::Glyph("📅"),
                        event: ModuleEvent::Command("DPR_DAY_ADD 2026-09-27 Block_Work 32.50 75 Completed masonry".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_DAY_REPORT",
                        label: "Daily Report",
                        icon: IconKind::Glyph("🗓"),
                        event: ModuleEvent::Command("DPR_DAY_REPORT".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_SET_PLAN",
                        label: "Set Plan",
                        icon: IconKind::Glyph("📐"),
                        event: ModuleEvent::Command("DPR_SET_PLAN Block_Work 180.0".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_PROGRESS",
                        label: "Progress",
                        icon: IconKind::Glyph("📊"),
                        event: ModuleEvent::Command("DPR_PROGRESS".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_DASHBOARD",
                        label: "Site Dashboard",
                        icon: IconKind::Glyph("▦"),
                        event: ModuleEvent::Command("DPR_DASHBOARD".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_ADD_TASK",
                        label: "Add Task",
                        icon: IconKind::Glyph("✓"),
                        event: ModuleEvent::Command("DPR_ADD_TASK Foundation 2026-09-27 2026-09-30 25".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_SCHEDULE",
                        label: "Schedule",
                        icon: IconKind::Glyph("🗓"),
                        event: ModuleEvent::Command("DPR_SCHEDULE".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_SET_PROJECT",
                        label: "Set Project",
                        icon: IconKind::Glyph("P"),
                        event: ModuleEvent::Command("DPR_SET_PROJECT IIIT Dharwad Hostel".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_SET_FLOOR",
                        label: "Set Floor",
                        icon: IconKind::Glyph("F"),
                        event: ModuleEvent::Command("DPR_SET_FLOOR Ground Floor".to_string()),
                    }),
                    RibbonItem::LargeTool(ToolDef {
                        id: "DPR_RESET",
                        label: "Reset",
                        icon: IconKind::Glyph("↺"),
                        event: ModuleEvent::Command("DPR_RESET".to_string()),
                    }),
                ],
            }]
        })
    }
}

struct DprPlugin;

#[derive(Clone, Copy)]
struct PickableLine {
    start: [f64; 2],
    end: [f64; 2],
}

struct PickWallCommand {
    lines: HashMap<Handle, PickableLine>,
    rectangles: HashMap<Handle, f64>,
    first_line: Option<PickableLine>,
    height_m: f64,
    drawing_unit: DrawingUnit,
    wall_thickness_m: f64,
}

impl InteractiveCommand for PickWallCommand {
    fn prompt(&self) -> String {
        if self.first_line.is_some() {
            format!(
                "Select the opposite parallel edge within {:.0} mm",
                self.wall_thickness_m * 1000.0
            )
        } else {
            format!(
                "Select a rectangular wall outline or its first parallel edge (height {:.2} m; units: {})",
                self.height_m,
                self.drawing_unit.label()
            )
        }
    }

    fn on_point(&mut self, _pt: [f64; 3]) -> CommandStep {
        CommandStep::Cancel
    }

    fn needs_object_pick(&self) -> bool {
        true
    }

    fn on_object_pick(&mut self, handle: Handle, _pt: [f64; 3]) -> CommandStep {
        if let Some(length_m) = self.rectangles.get(&handle).copied() {
            with_dpr_state(|state| {
                state.add_picked_wall(length_m, self.height_m);
            });
            return CommandStep::Done;
        }

        let Some(line) = self.lines.get(&handle).copied() else {
            return CommandStep::Cancel;
        };
        let Some(first_line) = self.first_line.replace(line) else {
            return CommandStep::NeedPoint;
        };
        if let Some(length_m) = paired_wall_length(first_line, line, self.wall_thickness_m) {
            with_dpr_state(|state| {
                state.add_picked_wall(length_m, self.height_m);
            });
            CommandStep::Done
        } else {
            CommandStep::NeedPoint
        }
    }
}

fn rectangle_long_side(points: &[[f64; 2]]) -> Option<f64> {
    if points.len() != 4 {
        return None;
    }
    let edges: Vec<[f64; 2]> = (0..4)
        .map(|index| [
            points[(index + 1) % 4][0] - points[index][0],
            points[(index + 1) % 4][1] - points[index][1],
        ])
        .collect();
    let lengths: Vec<f64> = edges
        .iter()
        .map(|edge| (edge[0] * edge[0] + edge[1] * edge[1]).sqrt())
        .collect();
    if lengths.iter().any(|length| *length <= f64::EPSILON) {
        return None;
    }

    for index in 0..4 {
        let current = edges[index];
        let next = edges[(index + 1) % 4];
        let dot = current[0] * next[0] + current[1] * next[1];
        if dot.abs() > lengths[index] * lengths[(index + 1) % 4] * 1e-6 {
            return None;
        }
    }
    for index in 0..2 {
        if (lengths[index] - lengths[index + 2]).abs() > lengths[index].max(lengths[index + 2]) * 1e-6 {
            return None;
        }
    }
    Some(lengths.into_iter().fold(0.0, f64::max))
}

fn paired_wall_length(first: PickableLine, second: PickableLine, thickness_m: f64) -> Option<f64> {
    let first_vector = [first.end[0] - first.start[0], first.end[1] - first.start[1]];
    let second_vector = [second.end[0] - second.start[0], second.end[1] - second.start[1]];
    let first_length = (first_vector[0].powi(2) + first_vector[1].powi(2)).sqrt();
    let second_length = (second_vector[0].powi(2) + second_vector[1].powi(2)).sqrt();
    if first_length <= f64::EPSILON || second_length <= f64::EPSILON {
        return None;
    }

    let cross = first_vector[0] * second_vector[1] - first_vector[1] * second_vector[0];
    if cross.abs() / (first_length * second_length) > 0.01 {
        return None;
    }
    if (first_length - second_length).abs() > (thickness_m * 0.1).max(0.001) {
        return None;
    }

    let offset = [second.start[0] - first.start[0], second.start[1] - first.start[1]];
    let separation = (first_vector[0] * offset[1] - first_vector[1] * offset[0]).abs() / first_length;
    let second_end_offset = [second.end[0] - first.start[0], second.end[1] - first.start[1]];
    let end_separation =
        (first_vector[0] * second_end_offset[1] - first_vector[1] * second_end_offset[0]).abs()
            / first_length;
    if separation <= 0.001
        || separation > thickness_m
        || end_separation <= 0.001
        || end_separation > thickness_m
    {
        return None;
    }

    let unit = [first_vector[0] / first_length, first_vector[1] / first_length];
    let second_start_projection = offset[0] * unit[0] + offset[1] * unit[1];
    let second_end_projection = second_end_offset[0] * unit[0] + second_end_offset[1] * unit[1];
    let endpoint_tolerance = (thickness_m * 0.1).max(0.001);
    let (low_projection, high_projection) = if second_start_projection <= second_end_projection {
        (second_start_projection, second_end_projection)
    } else {
        (second_end_projection, second_start_projection)
    };
    if low_projection.abs() > endpoint_tolerance
        || (high_projection - first_length).abs() > endpoint_tolerance
    {
        return None;
    }
    Some((first_length + second_length) * 0.5)
}

impl BuiltinPlugin for DprPlugin {
    fn manifest(&self) -> &'static PluginManifest {
        &MANIFEST
    }

    fn ribbon(&self) -> Box<dyn CadModule> {
        Box::new(DprModule)
    }

    fn dispatch(&self, host: &mut dyn HostApi, cmd: &str) -> bool {
        if cmd.trim() == "DPR_PICK_WALL" {
            let (height_m, drawing_unit, wall_thickness_m) = with_dpr_state(|state| {
                (state.wall_height_m, state.drawing_unit, state.wall_thickness_m)
            });
            let mut lines = HashMap::new();
            let mut rectangles = HashMap::new();
            for entity in host.document().entities() {
                match entity {
                    ocs_plugin_api::host::EntityType::Line(line) => {
                        let scale = drawing_unit.meters_per_unit();
                        lines.insert(
                            line.common.handle,
                            PickableLine {
                                start: [line.start.x * scale, line.start.y * scale],
                                end: [line.end.x * scale, line.end.y * scale],
                            },
                        );
                    }
                    ocs_plugin_api::host::EntityType::LwPolyline(polyline)
                        if polyline.is_closed
                            && polyline.vertices.iter().all(|vertex| vertex.bulge.abs() < 1e-9) =>
                    {
                        let points: Vec<[f64; 2]> = polyline
                            .vertices
                            .iter()
                            .map(|vertex| [vertex.location.x, vertex.location.y])
                            .collect();
                        if let Some(length) = rectangle_long_side(&points) {
                            rectangles.insert(
                                polyline.common.handle,
                                to_meters(length, drawing_unit),
                            );
                        }
                    }
                    ocs_plugin_api::host::EntityType::Polyline2D(polyline)
                        if polyline.flags.is_closed()
                            && polyline.vertices.iter().all(|vertex| vertex.bulge.abs() < 1e-9) =>
                    {
                        let points: Vec<[f64; 2]> = polyline
                            .vertices
                            .iter()
                            .map(|vertex| [vertex.location.x, vertex.location.y])
                            .collect();
                        if let Some(length) = rectangle_long_side(&points) {
                            rectangles.insert(
                                polyline.common.handle,
                                to_meters(length, drawing_unit),
                            );
                        }
                    }
                    _ => {}
                }
            }
            host.start_interactive(Box::new(PickWallCommand {
                lines,
                rectangles,
                first_line: None,
                height_m,
                drawing_unit,
                wall_thickness_m,
            }));
            return true;
        }

        if cmd.trim() == "DPR_SET_WALL_THICKNESS" {
            let thickness_mm = with_dpr_state(|state| state.wall_thickness_m * 1000.0);
            host.push_info(&format!(
                "Current wall thickness is {:.0} mm. Set with DPR_SET_WALL_THICKNESS <millimeters>.",
                thickness_mm
            ));
            return true;
        }

        if cmd.trim().starts_with("DPR_SET_WALL_THICKNESS") {
            match parse_set_wall_thickness_command(cmd) {
                Some(thickness_m) => {
                    with_dpr_state(|state| state.wall_thickness_m = thickness_m);
                    host.push_info(&format!(
                        "Wall thickness set to {:.0} mm",
                        thickness_m * 1000.0
                    ));
                }
                None => host.push_error("Usage: DPR_SET_WALL_THICKNESS <positive thickness in millimeters>"),
            }
            return true;
        }

        if cmd.trim() == "DPR_SET_UNITS" {
            let unit = with_dpr_state(|state| state.drawing_unit);
            host.push_info(&format!(
                "Current drawing units are {}. Set with DPR_SET_UNITS MM, CM, M, IN, or FT.",
                unit.label()
            ));
            return true;
        }

        if cmd.trim().starts_with("DPR_SET_UNITS") {
            match parse_set_drawing_unit_command(cmd) {
                Some(unit) => {
                    with_dpr_state(|state| state.drawing_unit = unit);
                    host.push_info(&format!(
                        "Drawing units set to {}. Picked line lengths will be converted to meters.",
                        unit.label()
                    ));
                }
                None => host.push_error("Usage: DPR_SET_UNITS MM, CM, M, IN, or FT"),
            }
            return true;
        }

        if cmd.trim() == "DPR_SET_WALL_HEIGHT" {
            let height_m = with_dpr_state(|state| state.wall_height_m);
            host.push_info(&format!(
                "Current wall height is {:.2} m. Set it with DPR_SET_WALL_HEIGHT <height>.",
                height_m
            ));
            return true;
        }

        if cmd.trim().starts_with("DPR_SET_WALL_HEIGHT") {
            match parse_set_wall_height_command(cmd) {
                Some(height_m) => {
                    with_dpr_state(|state| state.wall_height_m = height_m);
                    host.push_info(&format!("Wall height set to {:.2} m", height_m));
                }
                None => host.push_error("Usage: DPR_SET_WALL_HEIGHT <positive height in meters>"),
            }
            return true;
        }

        if let Some((wall_id, activity, length_m, height_m)) = parse_add_command(cmd) {
            with_dpr_state(|state| {
                state.add_measurement(wall_id, activity, length_m, height_m);
            });
            let area = calculate_wall_area(length_m, height_m);
            host.push_info(&format!(
                "Added {} | {} = {:.3} m²",
                wall_id, activity, area
            ));
            return true;
        }

        if let Some(project) = parse_set_project_command(cmd) {
            with_dpr_state(|state| state.set_project(project));
            host.push_info(&format!("Project set to {}", project));
            return true;
        }

        if let Some(floor) = parse_set_floor_command(cmd) {
            with_dpr_state(|state| state.set_floor(floor));
            host.push_info(&format!("Floor set to {}", floor));
            return true;
        }

        if let Some((activity, planned_m2)) = parse_set_plan_command(cmd) {
            with_dpr_state(|state| state.set_plan(activity, planned_m2));
            host.push_info(&format!(
                "Plan updated for {}: {:.3} m²",
                ActivityKind::from_str(activity).unwrap_or(ActivityKind::BlockWork).as_label(),
                planned_m2
            ));
            return true;
        }

        if let Some((date, activity, quantity, progress_pct, remarks)) = parse_daily_entry_command(cmd) {
            with_dpr_state(|state| {
                state.add_daily_entry(date, activity, quantity, progress_pct, &remarks);
            });
            host.push_info(&format!(
                "Daily DPR logged for {}: {} = {:.3} m² at {:.1}%",
                date,
                ActivityKind::from_str(activity).unwrap_or(ActivityKind::BlockWork).as_label(),
                quantity,
                progress_pct
            ));
            return true;
        }

        if let Some((title, start_date, end_date, progress_pct)) = parse_add_schedule_command(cmd) {
            with_dpr_state(|state| {
                state.add_schedule_item(title, start_date, end_date, progress_pct);
            });
            host.push_info(&format!(
                "Schedule updated: {} ({}, {}, {:.1}%)",
                title,
                start_date,
                end_date,
                progress_pct
            ));
            return true;
        }

        if let Some(path) = parse_save_command(cmd) {
            match with_dpr_state(|state| state.save_to_file(path)) {
                Ok(_) => host.push_info(&format!("DPR data saved to {}", path)),
                Err(err) => host.push_info(&format!("Failed to save DPR data: {}", err)),
            }
            return true;
        }

        if let Some(path) = parse_load_command(cmd) {
            match DprProjectState::load_from_file(path) {
                Ok(loaded) => {
                    with_dpr_state(|state| *state = loaded);
                    host.push_info(&format!("DPR data loaded from {}", path));
                }
                Err(err) => host.push_info(&format!("Failed to load DPR data: {}", err)),
            }
            return true;
        }

        if parse_daily_report_command(cmd) {
            let daily = with_dpr_state(|state| state.daily_table_text());
            host.push_info(&daily);
            return true;
        }

        if cmd == "DPR_SCHEDULE" {
            let schedule = with_dpr_state(|state| state.schedule_text());
            host.push_info(&schedule);
            return true;
        }

        if parse_progress_report_command(cmd) {
            let progress = with_dpr_state(|state| {
                if cmd == "DPR_DASHBOARD" || cmd == "DPR_SITE_DASHBOARD" {
                    state.dashboard_text()
                } else {
                    state.planned_vs_actual_text()
                }
            });
            host.push_info(&progress);
            return true;
        }

        match cmd {
            "DPR_SUMMARY" => {
                let summary = with_dpr_state(|state| state.summary_text());
                host.push_info(&summary);
                true
            }
            "DPR_EXPORT" => {
                let csv = with_dpr_state(|state| state.csv_text());
                host.push_info(&csv);
                true
            }
            "DPR_RESET" => {
                with_dpr_state(|state| {
                    state.entries.clear();
                    state.daily_entries.clear();
                    state.planned_entries.clear();
                });
                host.push_info("DPR measurement state was reset.");
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_vs_actual_percentage_is_calculated() {
        let mut state = DprProjectState::default();
        state.set_plan("Block Work", 100.0);
        state.add_measurement("W-01", "Block Work", 6.0, 3.0);

        let progress = state.planned_vs_actual_text();
        assert!(progress.contains("Completion 18.0%"));
    }

    #[test]
    fn picked_wall_uses_selected_length_and_configured_height() {
        let mut state = DprProjectState::default();
        let wall_id = state.add_picked_wall(4.25, 2.8);

        assert_eq!(wall_id, "W-01");
        assert_eq!(state.entries[0].length_m, 4.25);
        assert_eq!(state.entries[0].height_m, 2.8);
        assert!((state.entries[0].quantity_m2 - 11.9).abs() < 1e-9);
    }

    #[test]
    fn millimeter_drawing_length_converts_to_square_meters() {
        let length_m = to_meters(5460.0, DrawingUnit::Millimeters);
        let area_m2 = calculate_wall_area(length_m, 2.8);

        assert!((length_m - 5.46).abs() < 1e-9);
        assert!((area_m2 - 15.288).abs() < 1e-9);
    }

    #[test]
    fn drawing_units_parse_common_names_and_reject_unknown_units() {
        assert_eq!(parse_set_drawing_unit_command("DPR_SET_UNITS mm"), Some(DrawingUnit::Millimeters));
        assert_eq!(parse_set_drawing_unit_command("DPR_SET_UNITS FT"), Some(DrawingUnit::Feet));
        assert_eq!(parse_set_drawing_unit_command("DPR_SET_UNITS parsecs"), None);
    }

    #[test]
    fn wall_thickness_command_uses_millimeters() {
        assert_eq!(parse_set_wall_thickness_command("DPR_SET_WALL_THICKNESS 200"), Some(0.2));
        assert_eq!(parse_set_wall_thickness_command("DPR_SET_WALL_THICKNESS 0"), None);
    }

    #[test]
    fn closed_rectangle_returns_its_long_side() {
        let points = [[0.0, 0.0], [5460.0, 0.0], [5460.0, 200.0], [0.0, 200.0]];
        let long_side_m = to_meters(rectangle_long_side(&points).unwrap(), DrawingUnit::Millimeters);

        assert!((long_side_m - 5.46).abs() < 1e-9);
        assert!(rectangle_long_side(&[[0.0, 0.0], [5.0, 0.0], [4.0, 2.0], [0.0, 2.0]]).is_none());
    }

    #[test]
    fn parallel_edges_within_wall_thickness_form_a_wall() {
        let first = PickableLine {
            start: [0.0, 0.0],
            end: [5.46, 0.0],
        };
        let second = PickableLine {
            start: [0.0, 0.2],
            end: [5.46, 0.2],
        };

        assert!((paired_wall_length(first, second, 0.2).unwrap() - 5.46).abs() < 1e-9);
        assert!(paired_wall_length(first, second, 0.15).is_none());
    }

    #[test]
    fn wall_height_command_rejects_non_positive_or_non_finite_values() {
        assert_eq!(parse_set_wall_height_command("DPR_SET_WALL_HEIGHT 2.8"), Some(2.8));
        assert_eq!(parse_set_wall_height_command("DPR_SET_WALL_HEIGHT 0"), None);
        assert_eq!(parse_set_wall_height_command("DPR_SET_WALL_HEIGHT NaN"), None);
        assert_eq!(parse_set_wall_height_command("DPR_SET_WALL_HEIGHT"), None);
    }

    #[test]
    fn dashboard_contains_overall_completion_and_status() {
        let mut state = DprProjectState::default();
        state.set_plan("Block Work", 100.0);
        state.add_measurement("W-01", "Block Work", 6.0, 3.0);
        state.add_daily_entry("2026-09-27", "Block Work", 18.0, 18.0, "Masonry");

        let dashboard = state.dashboard_text();
        assert!(dashboard.contains("Site Dashboard"));
        assert!(dashboard.contains("Overall completion: 18.0%"));
        assert!(dashboard.contains("Status:"));
    }

    #[test]
    fn schedule_and_file_round_trip_work() {
        let path = std::env::temp_dir().join("dpr_schedule_roundtrip.txt");
        let mut state = DprProjectState::default();
        state.set_project("Project A");
        state.set_floor("Level 1");
        state.set_plan("Block Work", 200.0);
        state.add_measurement("W-01", "Block Work", 8.0, 3.0);
        state.add_schedule_item("Foundation", "2026-09-01", "2026-09-05", 30.0);

        state.save_to_file(path.to_str().unwrap()).unwrap();
        let loaded = DprProjectState::load_from_file(path.to_str().unwrap()).unwrap();

        assert_eq!(loaded.project, "Project A");
        assert_eq!(loaded.floor, "Level 1");
        assert_eq!(loaded.schedule.len(), 1);
        assert_eq!(loaded.schedule[0].title, "Foundation");

        let _ = std::fs::remove_file(path);
    }
}

ocs_plugin_api::export_plugin!(DprPlugin);
