//! Output formatting: table, JSON, YAML.

use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Table,
    Json,
    // Yaml is deferred until a yaml crate is added; fall back to JSON.
}

impl OutputFormat {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => Self::Json,
            _ => Self::Table,
        }
    }
}

/// Print a JSON value using the chosen format.
pub fn print_value(val: &Value, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(val).unwrap_or_default());
        }
        OutputFormat::Table => {
            // Fallback: pretty JSON if we can't build a table
            println!("{}", serde_json::to_string_pretty(val).unwrap_or_default());
        }
    }
}

/// Print a list of items as a table with the given columns.
/// `columns` is a list of (header, json_field_path) pairs.
pub fn print_table(items: &[Value], columns: &[(&str, &str)], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(items).unwrap_or_default());
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);

            // Header row
            table.set_header(columns.iter().map(|(h, _)| Cell::new(h).set_alignment(CellAlignment::Left)));

            for item in items {
                let row: Vec<Cell> = columns
                    .iter()
                    .map(|(_, field)| {
                        let val = extract_field(item, field);
                        Cell::new(format_cell(&val))
                    })
                    .collect();
                table.add_row(row);
            }

            println!("{table}");
        }
    }
}

/// Print a single record as key-value pairs.
pub fn print_record(item: &Value, fields: &[(&str, &str)], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(item).unwrap_or_default());
        }
        OutputFormat::Table => {
            let mut table = Table::new();
            table.set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(["Field", "Value"]);

            for (label, field) in fields {
                let val = extract_field(item, field);
                table.add_row([Cell::new(label), Cell::new(format_cell(&val))]);
            }

            println!("{table}");
        }
    }
}

fn extract_field<'a>(val: &'a Value, path: &str) -> &'a Value {
    let mut current = val;
    for part in path.split('.') {
        current = &current[part];
    }
    current
}

fn format_cell(val: &Value) -> String {
    match val {
        Value::Null => "—".into(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| format_cell(v)).collect();
            items.join(", ")
        }
        Value::Object(_) => serde_json::to_string(val).unwrap_or_default(),
    }
}
