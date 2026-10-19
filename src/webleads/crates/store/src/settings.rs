//! The few things the program must remember across a restart.
//!
//! Which area is chosen, whether the queue is paused, and the search language. A few
//! values, so a table of key and value rather than a table each — and in the database
//! rather than in memory because forgetting any of them on restart would be a surprise.

use sqlx::Row;

use crate::{Result, Store};

/// Which area the next search will look at.
pub const CHOSEN_AREA: &str = "chosen_area";

/// Whether the queue is holding rather than taking the next one.
pub const QUEUE_PAUSED: &str = "queue_paused";

/// The language searches are made in, as you last picked it.
pub const SEARCH_LANG: &str = "search_lang";

/// Your clock, as you last picked it.
pub const YOUR_ZONE: &str = "your_zone";

impl Store {
    /// One setting, or `None` when it has never been set.
    pub async fn setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM setting WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    /// Write one setting, replacing whatever was there.
    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO setting (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Is the queue holding?
    ///
    /// Anything other than a plain "yes" means no: a setting we cannot read is not a
    /// reason to stop working.
    pub async fn queue_paused(&self) -> Result<bool> {
        Ok(self.setting(QUEUE_PAUSED).await?.as_deref() == Some("yes"))
    }

    /// The language searches are made in: the one you last picked, or English.
    pub async fn search_lang(&self) -> Result<String> {
        Ok(self
            .setting(SEARCH_LANG)
            .await?
            .unwrap_or_else(|| webleads_domain::ScrapeOptions::default().lang))
    }

    /// Remember the language searches are made in. Only one the trade list is written
    /// in — anything else would search for nothing.
    pub async fn set_search_lang(&self, lang: &str) -> Result<()> {
        let lang = lang.trim().to_lowercase();
        if !webleads_domain::LANGUAGES
            .iter()
            .any(|(code, _)| *code == lang)
        {
            return Err(crate::Error::Refused(format!(
                "`{lang}` is not a language the trade list is written in"
            )));
        }
        self.set_setting(SEARCH_LANG, &lang).await
    }

    /// Your clock: the one you last picked, or the one a fresh install starts on.
    pub async fn your_zone(&self) -> Result<String> {
        Ok(self
            .setting(YOUR_ZONE)
            .await?
            .unwrap_or_else(|| webleads_domain::FIRST_ZONE.to_owned()))
    }

    /// Remember your clock. Only a zone the clock knows — anything else would put
    /// every ring-back an unknown number of hours out.
    pub async fn set_your_zone(&self, name: &str) -> Result<String> {
        let zone = webleads_domain::zone_named(name)
            .map(time_tz::TimeZone::name)
            .ok_or_else(|| {
                crate::Error::Refused(format!("`{}` is not a clock anybody keeps", name.trim()))
            })?;
        self.set_setting(YOUR_ZONE, zone).await?;
        Ok(zone.to_owned())
    }

    /// Hold the queue, or let it carry on.
    pub async fn pause_queue(&self, paused: bool) -> Result<()> {
        self.set_setting(QUEUE_PAUSED, if paused { "yes" } else { "no" })
            .await
    }
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
