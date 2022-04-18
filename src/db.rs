use anyhow::Error;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono;
use sqlx::{Pool, Sqlite};
use std::net::IpAddr;
use std::time::Duration;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub team_id: String,
    pub created_at: chrono::NaiveDateTime,
    pub num_pages: Option<i64>,
    pub process_time_ms: Option<i64>,
    pub failed: bool,
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

    pub async fn new_job(&self, team: &Team) -> Result<Job, Error> {
        let id = sqlx::query!("INSERT INTO jobs (team_id) VALUES (?)", team.team_id)
            .execute(self.db)
            .await?
            .last_insert_rowid();
        let job = sqlx::query_as!(Job, "SELECT * FROM jobs WHERE id = ?", id)
            .fetch_one(self.db)
            .await?;
        Ok(job)
    }

    pub async fn fail_job(&self, job: &Job) -> Result<(), Error> {
        sqlx::query!("UPDATE jobs set failed = true WHERE id = ?", job.id)
            .execute(self.db)
            .await?;
        Ok(())
    }

    pub async fn set_pages(&self, job: &Job, num_pages: usize) -> Result<(), Error> {
        let num_pages = num_pages as i64;
        sqlx::query!(
            "UPDATE jobs set num_pages = ? WHERE id = ?",
            num_pages,
            job.id
        )
        .execute(self.db)
        .await?;
        Ok(())
    }

    pub async fn set_process_time(&self, job: &Job, process_time: Duration) -> Result<(), Error> {
        let process_time_ms = process_time.as_millis() as i64;
        sqlx::query!(
            "UPDATE jobs set process_time_ms = ? WHERE id = ?",
            process_time_ms,
            job.id
        )
        .execute(self.db)
        .await?;
        Ok(())
    }
}
