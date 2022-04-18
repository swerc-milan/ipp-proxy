use std::path::{Path, PathBuf};
use tokio::fs::File;

use crate::Database;
use anyhow::{anyhow, bail, Error};
use futures_util::future::join_all;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::db::{Job, Team};

pub async fn process_pjl_message(
    db: &Database<'_>,
    team: &Team,
    job: &Job,
    payload: &[u8],
    jobs_dir: &Path,
) -> Result<Vec<u8>, Error> {
    let job_dir = jobs_dir.join(format!("job-{}", job.id));
    tokio::fs::create_dir_all(&job_dir).await?;
    write_to_file(&job_dir.join("original-payload.bin"), payload).await?;

    let pdf_start = payload
        .windows(4)
        .enumerate()
        .find(|&(_, w)| matches!(w, b"%PDF"))
        .map(|(i, _)| i)
        .ok_or_else(|| anyhow!("Cannot find PDF start"))?;
    let pjl_header = &payload[..pdf_start];
    let rest = &payload[pdf_start..];

    let pjl_footer_start = rest
        .windows(9)
        .enumerate()
        .find(|&(_, w)| matches!(w, b"\x1b%-12345X"))
        .map(|(i, _)| i)
        .unwrap_or(rest.len());
    let pdf = &rest[..pjl_footer_start];
    let pjl_footer = &rest[pjl_footer_start..];

    debug!(
        "{} bytes of header, {} bytes of PDF, {} bytes of footer",
        pjl_header.len(),
        pdf.len(),
        pjl_footer.len()
    );

    let original_pdf_path = job_dir.join("original-pdf.pdf");
    write_to_file(&original_pdf_path, pdf).await?;

    let patched_pdf_path = job_dir.join("patched-pdf.pdf");
    patch_pdf(&original_pdf_path, &patched_pdf_path, db, team, job).await?;

    let mut new_pdf_content = tokio::fs::read(&patched_pdf_path).await?;

    let mut new_payload: Vec<u8> = pjl_header.into();
    new_payload.append(&mut new_pdf_content);
    new_payload.extend_from_slice(pjl_footer);
    Ok(new_payload)
}

async fn write_to_file(path: &Path, payload: &[u8]) -> Result<(), Error> {
    let mut file = File::create(path).await?;
    file.write_all(payload).await?;
    Ok(())
}

async fn patch_pdf(
    source: &Path,
    target: &Path,
    db: &Database<'_>,
    team: &Team,
    job: &Job,
) -> Result<(), Error> {
    let start = std::time::Instant::now();
    let pages = split_pdf_pages(source).await?;
    let num_pages = pages.len();
    db.set_pages(job, num_pages).await?;

    let targets: Vec<_> = pages
        .iter()
        .map(|path| path.with_extension("patched.pdf"))
        .collect();
    let results = join_all(pages.iter().zip(targets.iter()).enumerate().map(
        |(page, (source, target))| {
            add_page_watermark(source.clone(), target.clone(), team, page, num_pages)
        },
    ))
    .await;
    for result in results {
        result?;
    }

    merge_pdf(&targets, target).await?;

    let process_time = start.elapsed();
    db.set_process_time(job, process_time).await?;

    Ok(())
}

async fn split_pdf_pages(source: &Path) -> Result<Vec<PathBuf>, Error> {
    let dir = source.parent().unwrap();
    let pattern = dir.join("page-%02d.pdf");
    let mut child = Command::new("pdftk")
        .arg(source)
        .arg("burst")
        .arg("output")
        .arg(pattern)
        .spawn()?;
    let exit_code = child.wait().await?;
    if !exit_code.success() {
        bail!("Failed to split pages");
    }
    let mut pages: Vec<PathBuf> = glob::glob(dir.join("page-*.pdf").to_string_lossy().as_ref())?
        .flatten()
        .collect();
    pages.sort();
    Ok(pages)
}

async fn add_page_watermark(
    source: PathBuf,
    target: PathBuf,
    team: &Team,
    page: usize,
    num_pages: usize,
) -> Result<(), Error> {
    let font_size = 12;
    let position_x = 50;
    let position_y = 20;
    let text = page_text(team, page, num_pages);

    let header_path = target.with_extension("header.pdf");
    let rotated_target = header_path.with_extension("rotated.pdf");
    let mut child = Command::new("gs")
        .arg("-o")
        .arg(&rotated_target)
        .arg("-sDEVICE=pdfwrite")
        .arg("-dDEVICEWIDTHPOINTS=842")
        .arg("-dDEVICEHEIGHTPOINTS=598")
        .arg("-c")
        .arg(format!("/Courier findfont {} scalefont setfont", font_size))
        .arg(format!(
            "{} {} moveto ({}) show showpage",
            position_x, position_y, text
        ))
        .spawn()?;
    let exit_code = child.wait().await?;
    if !exit_code.success() {
        bail!("Failed to build header with: {}", text);
    }

    let mut child = Command::new("pdftk")
        .arg(&rotated_target)
        .arg("cat")
        .arg("1-endwest")
        .arg("output")
        .arg(&header_path)
        .spawn()?;
    let exit_code = child.wait().await?;
    if !exit_code.success() {
        bail!("Failed to rotate {}", rotated_target.display());
    }

    let mut child = Command::new("pdftk")
        .arg(&source)
        .arg("background")
        .arg(&header_path)
        .arg("output")
        .arg(&target)
        .spawn()?;
    let exit_code = child.wait().await?;
    if !exit_code.success() {
        bail!("Failed to overlay {}", header_path.display());
    }

    Ok(())
}

fn page_text(team: &Team, page: usize, num_pages: usize) -> String {
    let name_limit = 30;
    let team_name = if team.team_name.len() < name_limit {
        format!("\"{}\"", team.team_name)
    } else {
        format!("\"{}...\"", &team.team_name[..name_limit])
    };
    let text = format!(
        "{} - Page {} of {} - Team {}",
        team.location,
        page + 1,
        num_pages,
        team_name
    )
    .replace(')', "\\)")
    .replace('(', "\\(");
    text
}

async fn merge_pdf(paths: &[PathBuf], target: &Path) -> Result<(), Error> {
    let mut cmd = Command::new("pdftk");
    for path in paths {
        cmd.arg(path);
    }
    let mut child = cmd.arg("cat").arg("output").arg(target).spawn()?;
    let exit_code = child.wait().await?;
    if !exit_code.success() {
        bail!("Failed to merge {}", target.display());
    }

    Ok(())
}
