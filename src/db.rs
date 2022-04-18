use anyhow::Error;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use std::net::IpAddr;

pub type Db = Pool<Sqlite>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub team_id: String,
    pub team_name: String,
    pub location: String,
    pub ip_address: Option<String>,
    pub password: Option<String>,
    pub ipp_upstream: String,
}

pub struct Database<'a> {
    db: &'a Db,
}

impl<'a> Database<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub async fn get_team(
        &self,
        ip_address: IpAddr,
        password: Option<&str>,
    ) -> Result<Team, Error> {
        let team = if let Some(password) = password {
            sqlx::query_as!(Team, "SELECT * FROM teams WHERE password = ?", password)
                .fetch_one(self.db)
                .await?
        } else {
            let ip_address = ip_address.to_string();
            sqlx::query_as!(Team, "SELECT * FROM teams WHERE ip_address = ?", ip_address)
                .fetch_one(self.db)
                .await?
        };
        Ok(team)
    }
}
