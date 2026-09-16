//! SPDX-License-Identifier: Apache-2.0
//! EMCOMM helpers: welfare codes, ICS-213 / radiogram compact forms, bulletin TTL.

use serde::{Deserialize, Serialize};

pub const BULLETIN_CHANNEL: &str = "#bulletin";
pub const BULLETIN_TTL: u8 = 6;
pub const BULLETIN_HOLD_HOURS: u64 = 168; // 7 days

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Welfare {
    Ok,
    NeedHelp,
    Medical,
    Shelter,
    Power,
    Water,
}

impl Welfare {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ok" | "fine" | "good" => Some(Self::Ok),
            "need-help" | "needhelp" | "help" => Some(Self::NeedHelp),
            "medical" | "med" => Some(Self::Medical),
            "shelter" => Some(Self::Shelter),
            "power" => Some(Self::Power),
            "water" => Some(Self::Water),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::NeedHelp => "need-help",
            Self::Medical => "medical",
            Self::Shelter => "shelter",
            Self::Power => "power",
            Self::Water => "water",
        }
    }

    pub fn code(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::NeedHelp => 1,
            Self::Medical => 2,
            Self::Shelter => 3,
            Self::Power => 4,
            Self::Water => 5,
        }
    }

    pub fn from_code(c: u8) -> Self {
        match c {
            1 => Self::NeedHelp,
            2 => Self::Medical,
            3 => Self::Shelter,
            4 => Self::Power,
            5 => Self::Water,
            _ => Self::Ok,
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Ok => "[OK]",
            Self::NeedHelp => "[HELP]",
            Self::Medical => "[MED]",
            Self::Shelter => "[SHL]",
            Self::Power => "[PWR]",
            Self::Water => "[H2O]",
        }
    }
}

/// Compact field-coded form. Wire body: `F|<kind>|<k=v;k=v>`
#[derive(Debug, Clone)]
pub struct Form {
    pub kind: FormKind,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Ics213,
    Radiogram,
}

impl FormKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ics213 => "ics213",
            Self::Radiogram => "rg",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ics213" | "213" => Some(Self::Ics213),
            "rg" | "radiogram" => Some(Self::Radiogram),
            _ => None,
        }
    }
}

impl Form {
    pub fn encode(&self) -> String {
        let fields: Vec<String> = self
            .fields
            .iter()
            .map(|(k, v)| format!("{k}={}", v.replace('|', "/").replace(';', ",")))
            .collect();
        format!("F|{}|{}", self.kind.as_str(), fields.join(";"))
    }

    pub fn decode(s: &str) -> Option<Self> {
        let rest = s.strip_prefix("F|")?;
        let (kind, fields) = rest.split_once('|')?;
        let kind = FormKind::parse(kind)?;
        let fields = fields
            .split(';')
            .filter_map(|p| {
                p.split_once('=')
                    .map(|(k, v)| (k.to_string(), v.to_string()))
            })
            .collect();
        Some(Self { kind, fields })
    }

    pub fn render_text(&self) -> String {
        let title = match self.kind {
            FormKind::Ics213 => "ICS-213 GENERAL MESSAGE",
            FormKind::Radiogram => "RADIOGRAM",
        };
        let mut out = format!("{title}\n");
        for (k, v) in &self.fields {
            out.push_str(&format!("  {k}: {v}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_roundtrip() {
        let f = Form {
            kind: FormKind::Ics213,
            fields: vec![
                ("from".into(), "G4ABC".into()),
                ("to".into(), "M0XYZ".into()),
                ("msg".into(), "need generator".into()),
            ],
        };
        let s = f.encode();
        let back = Form::decode(&s).unwrap();
        assert_eq!(back.kind, FormKind::Ics213);
        assert_eq!(back.fields.len(), 3);
    }
}
