//! Auxiliary bearer services: logtime stats (`translate`), attendance
//! (`edtrax`) and the pace system.

use std::time::Duration;

use super::{Api, ApiResult};

const TTL_LOGTIME: Duration = Duration::from_secs(5 * 60);

impl Api {
    /// Per-day logtime as `{"YYYY-MM-DD": "HH:MM:SS"}`.
    pub async fn locations_stats(
        &self,
        login: &str,
        fresh: bool,
    ) -> ApiResult<super::LocationStats> {
        let key = format!("logtime/{login}");
        if !fresh && let Some(cached) = self.cache.get(&key, TTL_LOGTIME) {
            return Ok(cached);
        }
        let stats: super::LocationStats = self
            .authed_get(&format!(
                "{}/users/{login}/locations_stats/",
                super::TRANSLATE_BASE
            ))
            .await?;
        self.cache.put(&key, &stats);
        Ok(stats)
    }

    /// Weekly attendance totals for the last four weeks.
    pub async fn attendance_summary(&self, user_id: u32) -> ApiResult<super::AttendanceSummary> {
        self.authed_get(&format!(
            "{}/attendance/{user_id}/summary",
            super::EDTRAX_BASE
        ))
        .await
    }

    /// Pace profile: milestones, deadlines and pace percentage.
    pub async fn pace_profile(&self, user_id: u32) -> ApiResult<super::PaceProfile> {
        let key = format!("pace/{user_id}");
        if let Some(cached) = self
            .cache
            .get::<super::PaceProfile>(&key, Duration::from_secs(30 * 60))
        {
            return Ok(cached);
        }
        if let Some((cached, _age)) = self.cache.get_with_age::<super::PaceProfile>(&key)
            && (cached.is_activated || cached.milestone.is_some())
        {
            return Ok(cached);
        }
        let value: super::PaceProfile = self
            .authed_get(&format!("{}/users/{user_id}/profile", super::PACE_BASE))
            .await?;
        self.cache.put(&key, &value);
        Ok(value)
    }
}
