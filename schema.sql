CREATE TABLE teams (
    team_id VARCHAR(128) PRIMARY KEY NOT NULL,
    team_name TEXT NOT NULL,
    location TEXT NOT NULL,
    ip_address TEXT,
    password TEXT,
    ipp_upstream TEXT NOT NULL
);

CREATE TABLE jobs (
    id INTEGER PRIMARY KEY NOT NULL,
    team_id VARCHAR(128) NOT NULL REFERENCES teams(team_id),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    num_pages INTEGER,
    process_time_ms INTEGER,
    failed BOOLEAN NOT NULL DEFAULT false
);

INSERT INTO teams (team_id, team_name, location, password, ipp_upstream)
VALUES ("edomora97", "Edoardo's team", "B6.2.6 - Row 3 - PC 1", "edoedoedo", "localhost:631/printers/Virtual_PDF_Printer");