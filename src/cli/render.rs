use crate::engine::deduplicator::DuplicateGroup;
use crate::engine::organizer::OrganizePlan;
use crate::models::media::{MediaItem, RenamePlan};
use indicatif::HumanBytes;

pub fn print_dedup_table(groups: &[DuplicateGroup], items: &[MediaItem]) {
    use console::style;

    for (gi, group) in groups.iter().enumerate() {
        println!(
            "\n{}",
            style(format!(
                "Group {} — {} ({:?})",
                gi + 1,
                group.content_id,
                group.kind
            ))
            .bold()
            .yellow()
        );
        println!("  {}", group.summary);
        for guard in &group.guardrails {
            println!("  guard: {}", super::commands::truncate(guard, 90));
        }

        println!(
            "  {}  {}  {}  {}  {}  {}  {}",
            style("Keep").bold().cyan().dim(),
            style("Path").bold().cyan().dim(),
            style("Size").bold().cyan().dim(),
            style("Quality").bold().cyan().dim(),
            style("Meta").bold().cyan().dim(),
            style("Score").bold().cyan().dim(),
            style("Why").bold().cyan().dim(),
        );

        for di in &group.items {
            let item = &items[di.index];
            let keep = if di.is_keep { "✓ KEEP" } else { "✗ REMOVE" };
            let score = if let Some(q) = &item.quality {
                format!("{:.1}", q.quality_score)
            } else {
                "—".into()
            };
            let quality = item
                .quality
                .as_ref()
                .map(|q| q.resolution_label.clone())
                .unwrap_or_default();

            println!(
                "  {:<10} {:<40} {:<10} {:<10} {:<6} {:<6} {}",
                keep,
                super::commands::truncate(&item.path.display().to_string(), 40),
                HumanBytes(item.file_size).to_string(),
                quality,
                format!("{:.2}", di.metadata_confidence),
                score,
                super::commands::truncate(&di.rationale, 42),
            );
        }
    }
}

pub fn print_rename_table(plans: &[RenamePlan]) {
    use console::style;

    println!(
        "{}  {}  {}",
        style("Old").bold().cyan().dim(),
        style("→").bold().yellow().dim(),
        style("New").bold().green().dim(),
    );

    for plan in plans {
        let old_name = plan
            .old_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let new_name = plan
            .new_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        println!(
            "  {} → {}",
            super::commands::truncate(&old_name, 50),
            super::commands::truncate(&new_name, 50)
        );
        if !plan.rationale.is_empty() {
            println!(
                "    rationale: {}",
                super::commands::truncate(&plan.rationale.join(" | "), 90)
            );
        }
        for conflict in &plan.conflicts {
            println!("    conflict: {}", super::commands::truncate(conflict, 90));
        }

        for sub in &plan.subtitle_plans {
            let sub_old = sub
                .old_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let sub_new = sub
                .new_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            println!(
                "  {} → {}  (subtitle)",
                super::commands::truncate(&sub_old, 48),
                super::commands::truncate(&sub_new, 48)
            );
        }
    }
}

pub fn print_organize_table(plans: &[OrganizePlan]) {
    use console::style;

    println!(
        "{}  {}  {}  {}  {}",
        style("Action").bold().cyan().dim(),
        style("Source").bold().cyan().dim(),
        style("→").bold().yellow().dim(),
        style("Target").bold().green().dim(),
        style("Extras").bold().cyan().dim(),
    );

    for plan in plans {
        let action = format!("{:?}", plan.action).to_lowercase();
        let src_name = plan
            .source
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let tgt_dir = plan
            .target
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let mut extras: Vec<String> = Vec::new();
        if plan.nfo_content.is_some() {
            extras.push("nfo".into());
        }
        if !plan.image_urls.is_empty() {
            extras.push(format!("{}img", plan.image_urls.len()));
        }

        println!(
            "  {:<10} {} → {}/  {}",
            action,
            super::commands::truncate(&src_name, 35),
            super::commands::truncate(&tgt_dir, 35),
            extras.join("+"),
        );
        if !plan.rationale.is_empty() {
            println!(
                "    rationale: {}",
                super::commands::truncate(&plan.rationale.join(" | "), 90)
            );
        }
        for conflict in &plan.conflicts {
            println!("    conflict: {}", super::commands::truncate(conflict, 90));
        }
    }
}
