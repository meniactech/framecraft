// SPDX-License-Identifier: AGPL-3.0-or-later
//! Project and graphic packs: FrameCraft writes the same project packs as
//! FRAME, and reads its own packs back unchanged.

mod common;

use framecraft_core::normalize::{Document, normalize};
use framecraft_core::pack::{Pack, PackSource, read_pack, write_graphic_pack, write_project_pack};
use framecraft_core::profile::Profile;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const EXPORTED_AT: &str = "2026-01-01T12:00:00.000Z";

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn project_packs_match_frame_and_round_trip() {
    let Some(expected) = common::expected_dir() else {
        return;
    };
    let reference = expected.parent().unwrap();
    let static_dir = reference.parent().unwrap().join("static");
    let p = Profile::frame();
    let mut failures = Vec::new();

    let mut packs: Vec<PathBuf> = std::fs::read_dir(reference.join("projects"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(".frameproject.zip"))
        .collect();
    packs.sort();

    for pack_path in packs {
        let pack_name = pack_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".frameproject.zip", "");
        let (Pack::Project(pack), _) = read_pack(&std::fs::read(&pack_path).unwrap()).unwrap()
        else {
            panic!("{pack_name}: not a project pack")
        };
        let docs: Vec<Document> = pack
            .graphics
            .iter()
            .map(|g| normalize(&g.graphic, &p))
            .collect();
        let name = pack.name.clone();
        let media = common::PackMedia {
            pack,
            static_dir: static_dir.clone(),
        };
        let sources: Vec<PackSource> = media
            .pack
            .graphics
            .iter()
            .zip(&docs)
            .map(|(g, doc)| PackSource {
                id: &g.id,
                name: &g.name,
                doc,
                thumbnail: None,
            })
            .collect();
        let written = write_project_pack(&name, &sources, &media, &[], None, EXPORTED_AT);
        assert!(
            written.missing.is_empty(),
            "{pack_name}: missing {:?}",
            written.missing
        );

        // Same files as FRAME's rebuilt pack: JSON by value, the rest by hash.
        let dir = expected.join(&pack_name).join("_project-pack");
        let want = common::read_json(&dir.join("files.json"));
        let want = want.as_object().unwrap();
        let got_paths: Vec<&String> = written.files.iter().map(|(p, _)| p).collect();
        let mut want_paths: Vec<&String> = want.keys().collect();
        let mut sorted_got = got_paths.clone();
        sorted_got.sort();
        want_paths.sort();
        if sorted_got != want_paths {
            failures.push(format!(
                "{pack_name}: files {sorted_got:?}, want {want_paths:?}"
            ));
        }
        for (path, bytes) in &written.files {
            let Some(w) = want.get(path) else { continue };
            // Graphics and the manifest compare by value; media (Lottie JSON too) by hash.
            if w["stored"] == true && path.ends_with(".json") {
                let want_json = common::read_json(&dir.join(path));
                let got_json: serde_json::Value = serde_json::from_slice(bytes).unwrap();
                let diffs = common::json_diffs(&want_json, &got_json);
                failures.extend(
                    diffs
                        .into_iter()
                        .take(10)
                        .map(|d| format!("{pack_name}/{path}: {d}")),
                );
            } else if w["sha256"] != sha256(bytes) {
                failures.push(format!("{pack_name}/{path}: content differs"));
            }
        }

        // FrameCraft reads its own pack back unchanged.
        let (Pack::Project(again), warnings) = read_pack(&written.to_zip().unwrap()).unwrap()
        else {
            panic!("{pack_name}: written pack is not a project pack")
        };
        assert!(warnings.is_empty(), "{pack_name}: {warnings:?}");
        assert_eq!(again.name, name);
        assert_eq!(again.graphics.len(), docs.len());
        for (g, doc) in again.graphics.iter().zip(&docs) {
            assert_eq!(
                g.graphic,
                doc.to_graphic(),
                "{pack_name}/{}: graphic changed in round trip",
                g.name
            );
        }
        for m in &media.pack.media {
            let bytes = again
                .media
                .iter()
                .find(|a| a.entry.url == m.entry.url)
                .map(|a| &a.bytes);
            assert_eq!(
                bytes,
                Some(&m.bytes),
                "{pack_name}: media {} changed",
                m.entry.url
            );
        }

        // A single-graphic pack of the graphic with the most media round-trips too.
        let (g, doc) = media
            .pack
            .graphics
            .iter()
            .zip(&docs)
            .max_by_key(|(_, d)| d.items.iter().filter(|i| i.media_url.is_some()).count())
            .unwrap();
        let gp = write_graphic_pack(&g.name, doc, &media, &[], Some(b"thumb"), EXPORTED_AT);
        let (Pack::Graphic(gp), warnings) = read_pack(&gp.to_zip().unwrap()).unwrap() else {
            panic!("{pack_name}: written graphic pack is not a graphic pack")
        };
        assert!(warnings.is_empty());
        assert_eq!(gp.graphic.name, g.name);
        assert_eq!(gp.graphic.graphic, doc.to_graphic());
        assert_eq!(gp.graphic.thumbnail.as_deref(), Some(&b"thumb"[..]));
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
