//! `intrapy.intra.42.fr/api/v1` — users, cursus, projects, events,
//! notifications, scale teams and achievements (bearer token).

use std::time::Duration;

use super::{Api, ApiResult, MeSummary};

const TTL_PROFILE: Duration = Duration::from_secs(10 * 60);
const TTL_EVENTS: Duration = Duration::from_secs(5 * 60);

impl Api {
    pub async fn me_summary(&self, fresh: bool) -> ApiResult<MeSummary> {
        let url = format!("{}/users/me/summary", super::INTRAPY_BASE);
        if !fresh && let Some(cached) = self.cache.get_stale("me/summary") {
            return Ok(cached);
        }
        let summary: MeSummary = self.authed_get(&url).await?;
        self.cache.put("me/summary", &summary);
        Ok(summary)
    }

    /// Full user record by login (or numeric id as string).
    pub async fn user_profile(&self, login: &str, fresh: bool) -> ApiResult<super::UserProfile> {
        let key = format!("users/{login}");
        if !fresh && let Some(cached) = self.cache.get(&key, TTL_PROFILE) {
            return Ok(cached);
        }
        let profile: super::UserProfile = self
            .authed_get(&format!("{}/users/{login}", super::INTRAPY_BASE))
            .await?;
        self.cache.put(&key, &profile);
        Ok(profile)
    }

    pub async fn user_cursus(&self, user_id: u32, fresh: bool) -> ApiResult<Vec<super::Cursus>> {
        let key = format!("users/{user_id}/cursus");
        if !fresh && let Some(cached) = self.cache.get(&key, TTL_PROFILE) {
            return Ok(cached);
        }
        let cursus: Vec<super::Cursus> = self
            .authed_get(&format!("{}/users/{user_id}/cursus", super::INTRAPY_BASE))
            .await?;
        self.cache.put(&key, &cursus);
        Ok(cursus)
    }

    pub async fn user_campus(&self, user_id: u32) -> ApiResult<Vec<super::Campus>> {
        self.authed_get(&format!("{}/users/{user_id}/campus", super::INTRAPY_BASE))
            .await
    }

    pub async fn user_achievements(&self, user_id: u32) -> ApiResult<Vec<super::Achievement>> {
        self.authed_get(&format!(
            "{}/users/{user_id}/achievements",
            super::INTRAPY_BASE
        ))
        .await
    }

    pub async fn ongoing_projects(
        &self,
        user_id: u32,
        cursus_id: u32,
    ) -> ApiResult<Vec<super::OngoingProject>> {
        self.authed_get(&format!(
            "{}/users/{user_id}/projects/ongoing?cursus_id={cursus_id}",
            super::INTRAPY_BASE
        ))
        .await
    }

    pub async fn marked_projects(
        &self,
        login: &str,
        cursus_id: u32,
    ) -> ApiResult<Vec<super::MarkedProject>> {
        self.authed_get(&format!(
            "{}/users/{login}/projects/marked?cursus_id={cursus_id}",
            super::INTRAPY_BASE
        ))
        .await
    }

    pub async fn my_events(&self, fresh: bool) -> ApiResult<Vec<super::IntraEvent>> {
        if !fresh && let Some(cached) = self.cache.get("me/events", TTL_EVENTS) {
            return Ok(cached);
        }
        let events: Vec<super::IntraEvent> = self
            .authed_get(&format!("{}/users/me/events", super::INTRAPY_BASE))
            .await?;
        self.cache.put("me/events", &events);
        Ok(events)
    }

    pub async fn my_scale_teams(&self) -> ApiResult<Vec<super::ScaleTeam>> {
        self.authed_get(&format!("{}/users/me/scale_teams", super::INTRAPY_BASE))
            .await
    }

    /// Unread + recent read notifications in one payload.
    pub async fn my_notifications(&self) -> ApiResult<super::NotificationsPayload> {
        let base = format!("{}/users/me/notifications", super::INTRAPY_BASE);
        let unread_value: serde_json::Value = self.authed_get(&format!("{base}/unread")).await?;
        let unread = unread_value["count"].as_u64().unwrap_or(0) as usize;
        let mut items = match unread_value.get("notifications") {
            Some(serde_json::Value::Array(list)) => list
                .clone()
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect::<Vec<super::Notification>>(),
            _ => Vec::new(),
        };
        if let Ok(read) = self
            .authed_get::<Vec<super::Notification>>(&format!("{base}/read"))
            .await
        {
            items.extend(read);
        }
        Ok(super::NotificationsPayload { unread, items })
    }

    pub async fn user_patroning(&self, login: &str) -> ApiResult<Vec<super::PatronUser>> {
        self.authed_get(&format!("{}/users/{login}/patroning", super::INTRAPY_BASE))
            .await
    }

    pub async fn user_patroned(&self, login: &str) -> ApiResult<Vec<super::PatronUser>> {
        self.authed_get(&format!("{}/users/{login}/patroned", super::INTRAPY_BASE))
            .await
    }
}
