use tracedraw_thumbnailer::{CLSID_TEXT, PROPERTY_CLSID_TEXT};

fn repo_file(path: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path);
    std::fs::read_to_string(&path).expect("repository registration file")
}

#[test]
fn windows_setup_registers_the_rating_handler_for_all_users() {
    let setup = repo_file("packaging/windows/tracedraw.iss");
    let class = PROPERTY_CLSID_TEXT.replacen('{', "{{", 1);
    let lines: Vec<_> = setup.lines().map(str::trim).collect();
    let contains = |parts: &[&str]| {
        lines
            .iter()
            .any(|line| parts.iter().all(|part| line.contains(part)))
    };
    let class_key = format!("Software\\Classes\\CLSID\\{class}");
    assert!(contains(&[
        "Root: HKLM",
        &format!("Subkey: \"{class_key}\\InprocServer32\""),
        "ValueName: \"\"",
        "ValueData: \"{app}\\tracedraw_thumbnailer.dll\"",
        "Check: IsAdminInstallMode",
    ]));
    assert!(contains(&[
        &format!("Subkey: \"{class_key}\\InprocServer32\""),
        "ValueName: \"ThreadingModel\"",
        "ValueData: \"Both\"",
        "Check: IsAdminInstallMode",
    ]));
    assert!(contains(&[
        "Root: HKLM",
        "Subkey: \"Software\\Microsoft\\Windows\\CurrentVersion\\PropertySystem\\PropertyHandlers\\.tdraw\"",
        &format!("ValueData: \"{class}\""),
        "uninsdeletevalue",
        "Check: IsAdminInstallMode",
    ]));
    for owner in ["TraceDraw.Document", "SystemFileAssociations\\.tdraw"] {
        for list in ["FullDetails", "PreviewDetails", "InfoTip"] {
            assert!(contains(&[
                "Root: HKLM",
                &format!("Subkey: \"Software\\Classes\\{owner}\""),
                &format!("ValueName: \"{list}\""),
                "System.Rating",
                "Check: IsAdminInstallMode",
            ]));
        }
    }
    assert_ne!(PROPERTY_CLSID_TEXT, CLSID_TEXT);
}

#[test]
fn store_manifest_declares_the_rating_class_and_its_details_list() {
    let manifest = repo_file("packaging/windows/msix/AppxManifest.xml");
    let class = PROPERTY_CLSID_TEXT.trim_matches(['{', '}']);
    let association = |name| {
        manifest
            .split(&format!("<uap:FileTypeAssociation Name=\"{name}\">"))
            .nth(1)
            .and_then(|text| text.split("</uap:FileTypeAssociation>").next())
            .expect("file association")
    };
    let native = association("tdraw");
    assert!(native.contains(&format!(
        "<desktop2:DesktopPropertyHandler Clsid=\"{class}\""
    )));
    for list in ["fullDetails", "previewDetails"] {
        assert!(native.lines().any(|line| {
            line.contains(&format!("Property=\"{list}\"")) && line.contains("System.Rating")
        }));
    }
    assert!(manifest.contains(&format!(
        "<com:Class Id=\"{class}\" Path=\"tracedraw_thumbnailer.dll\" ThreadingModel=\"Both\""
    )));
    assert!(!association("cdr").contains("DesktopPropertyHandler"));
    assert!(native.contains("desktop2:ThumbnailHandler"));
}
