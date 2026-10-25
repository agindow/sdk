use super::{Error, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertAction {
    Disabled,
    Individual,
    CreateSummary { consolidated: u64 },
    UpdateSummary { consolidated: u64 },
}
/// One owner per (tenant, connection), shared across endpoints/fields. The host
/// serializes updates and persists the summary; this library never sends alerts.
#[derive(Debug)]
pub struct AlertBudget {
    tenant: String,
    connection: String,
    cap: u32,
    day: Option<u64>,
    notifications: u32,
    consolidated: u64,
}
impl AlertBudget {
    pub fn new(tenant: &str, connection: &str, cap: u32) -> Result<Self, Error> {
        text(tenant)?;
        text(connection)?;
        Ok(Self {
            tenant: tenant.into(),
            connection: connection.into(),
            cap,
            day: None,
            notifications: 0,
            consolidated: 0,
        })
    }
    pub fn scope(&self) -> (&str, &str) {
        (&self.tenant, &self.connection)
    }
    pub fn notifications(&self) -> u32 {
        self.notifications
    }
    /// Record only eligible findings. `day` is UTC epoch seconds / 86_400.
    /// UpdateSummary updates the reserved notification, never sends another.
    pub fn record(&mut self, day: u64) -> Result<AlertAction, Error> {
        if self.day.is_some_and(|prior| day < prior) {
            return Err(Error::StaleDay);
        }
        if self.day != Some(day) {
            self.day = Some(day);
            self.notifications = 0;
            self.consolidated = 0;
        }
        if self.cap == 0 {
            return Ok(AlertAction::Disabled);
        }
        if self.notifications < self.cap - 1 {
            self.notifications += 1;
            return Ok(AlertAction::Individual);
        }
        self.consolidated = self.consolidated.saturating_add(1);
        if self.notifications < self.cap {
            self.notifications += 1;
            Ok(AlertAction::CreateSummary {
                consolidated: self.consolidated,
            })
        } else {
            Ok(AlertAction::UpdateSummary {
                consolidated: self.consolidated,
            })
        }
    }
}
