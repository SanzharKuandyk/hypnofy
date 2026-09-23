use super::*;

impl Store {
    pub fn load_app_settings(&self, defaults: AppSettings) -> Result<AppSettings, String> {
        let conn = self.conn()?;
        let stored = conn
            .query_row(
                "SELECT value FROM settings WHERE key='app_preferences'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(err)?;
        match stored {
            Some(json) => serde_json::from_str(&json)
                .map_err(|error| format!("Saved application settings are invalid: {error}")),
            None => Ok(defaults),
        }
    }

    pub fn save_app_settings(&self, settings: &AppSettings) -> Result<(), String> {
        settings.validate()?;
        let json = serde_json::to_string(settings).map_err(err)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO settings(key,value) VALUES('app_preferences',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [json],
        )
        .map_err(err)?;
        Ok(())
    }
}
