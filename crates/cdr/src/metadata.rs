//! Rating properties in the XML summary of ZIP drawing packages.
//!
//! The property bag and the rating's key were observed in a drawing made
//! by TraceDraw and rated through the public Windows property APIs. The
//! source and the package members are recorded in CLEANROOM.md and
//! docs/cdr-format.md.

use crate::container::{self, Container};

pub(crate) const PROPERTY_NS: &str = "http://namespace.corel.com/zcf/metadata/1.0/";
const SUMMARY_NS: &str = "http://namespace.corel.com/Cdr/metadata";
const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const RATING_FMTID: &str = "64440492-4c8b-11d1-8b70-080036b11a03";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;

/// Missing, unsupported and damaged metadata leaves the drawing readable.
pub(crate) fn read_rating(bytes: &[u8], container: &Container) -> Option<u8> {
    let member = if container.version.0 < 16 {
        "metadata/metadata.xml"
    } else {
        "META-INF/metadata.xml"
    };
    let xml = container::own_member(bytes, container, member, MAX_METADATA_BYTES)?;
    rating_from_xml(&xml, container.version.0)
}

fn rating_from_xml(xml: &[u8], version: u16) -> Option<u8> {
    if xml.len() as u64 > MAX_METADATA_BYTES {
        return None;
    }
    let text = std::str::from_utf8(xml).ok()?;
    // The default parser rejects DTDs. Nothing in the file may cause an
    // external resource to be loaded or an entity expansion to be attempted.
    let document = roxmltree::Document::parse(text).ok()?;
    if version < 16 {
        return document
            .descendants()
            .filter(|node| node.has_tag_name((SUMMARY_NS, "Properties")))
            .flat_map(|node| node.children())
            .filter(|node| node.has_tag_name((SUMMARY_NS, "Property")))
            .find_map(|node| {
                property_rating(
                    node.attribute("fmtid")?,
                    node.attribute("propid")?,
                    node.attribute("type")?,
                    node.attribute("array")?,
                    node.text()?,
                )
            });
    }
    document
        .descendants()
        .filter(|node| node.has_tag_name((PROPERTY_NS, "OpenMetadata")))
        .flat_map(|node| node.children())
        .filter(|node| node.has_tag_name((RDF_NS, "Bag")))
        .flat_map(|node| node.children())
        .filter(|node| node.has_tag_name((RDF_NS, "li")))
        .find_map(|node| {
            property_rating(
                node.attribute((PROPERTY_NS, "fmtid"))?,
                node.attribute((PROPERTY_NS, "propid"))?,
                node.attribute((PROPERTY_NS, "type"))?,
                node.attribute((PROPERTY_NS, "array"))?,
                node.attribute((PROPERTY_NS, "value"))?,
            )
        })
}

fn property_rating(fmtid: &str, id: &str, kind: &str, array: &str, value: &str) -> Option<u8> {
    if !fmtid
        .trim()
        .trim_matches(['{', '}'])
        .eq_ignore_ascii_case(RATING_FMTID)
        || id.parse::<u32>().ok()? != 9
        || kind != "uint"
        || array != "false"
    {
        return None;
    }
    document_rating(value.trim().parse().ok()?)
}

fn document_rating(value: u32) -> Option<u8> {
    Some(match value {
        0 => 0,
        1..=12 => 1,
        13..=37 => 2,
        38..=62 => 3,
        63..=87 => 4,
        88..=99 => 5,
        _ => return None,
    })
}

fn shell_rating(rating: u8) -> u32 {
    match rating.min(5) {
        0 => 0,
        1 => 1,
        2 => 25,
        3 => 50,
        4 => 75,
        _ => 99,
    }
}

/// The summary's surrounding Description declares the property namespace.
pub(crate) fn rating_xml(rating: u8, version: u16) -> String {
    if version < 16 {
        return format!(
            "            <Properties>\n                <Property array=\"false\" fmtid=\"{{{}}}\" propid=\"9\" type=\"uint\">{}</Property>\n            </Properties>\n",
            RATING_FMTID.to_ascii_uppercase(),
            shell_rating(rating),
        );
    }
    format!(
        "            <crl:OpenMetadata>\n                <rdf:Bag>\n                    <rdf:li crl:array=\"false\" crl:fmtid=\"{RATING_FMTID}\" crl:propid=\"9\" crl:type=\"uint\" crl:value=\"{}\" />\n                </rdf:Bag>\n            </crl:OpenMetadata>\n",
        shell_rating(rating)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};
    use tracedraw_core::Document;

    fn wrap(entries: &str) -> String {
        format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><r:RDF xmlns:r="{RDF_NS}"><r:Description xmlns:p="{PROPERTY_NS}"><p:OpenMetadata><r:Bag>{entries}</r:Bag></p:OpenMetadata></r:Description></r:RDF></x:xmpmeta>"#
        )
    }

    fn entry(value: &str) -> String {
        format!(
            r#"<r:li p:array="false" p:fmtid="{RATING_FMTID}" p:propid="9" p:type="uint" p:value="{value}" />"#
        )
    }

    /// Rewrite only metadata in a generated package, including its absence.
    fn replace_metadata(bytes: &[u8], version: u16, xml: Option<&[u8]>) -> Vec<u8> {
        let member = if version < 16 {
            "metadata/metadata.xml"
        } else {
            "META-INF/metadata.xml"
        };
        let mut source = zip::ZipArchive::new(Cursor::new(bytes)).expect("package");
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..source.len() {
            let mut file = source.by_index(index).expect("member");
            let name = file.name().expect("name").to_string();
            if name == member {
                if let Some(xml) = xml {
                    output
                        .start_file(name, zip::write::SimpleFileOptions::default())
                        .expect("metadata member");
                    output.write_all(xml).expect("metadata bytes");
                }
                continue;
            }
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("member data");
            output
                .start_file(name, zip::write::SimpleFileOptions::default())
                .expect("member header");
            output.write_all(&data).expect("member data");
        }
        output.finish().expect("package written").into_inner()
    }

    #[test]
    fn cdr_zip_ratings_round_trip_in_versions_14_through_27() {
        let mut doc = Document::default();
        doc.title = "Rating sample".into();
        for version in 14..=27 {
            for rating in 0..=5 {
                doc.metadata.rating = rating;
                let bytes = crate::write::document_to_cdr_version(&doc, version);
                let (back, _) = crate::open_bytes(&bytes, "File name").expect("read drawing");
                assert_eq!(
                    back.metadata.rating, rating,
                    "version {version}, {rating} stars"
                );
                assert_eq!(back.title, "File name", "existing title behavior");
                assert_eq!(back.pages.len(), doc.pages.len());
            }
        }
    }

    #[test]
    fn cdr_rating_keys_use_namespace_uris_and_ignore_unrelated_properties() {
        let other = entry("75").replace("propid=\"9\"", "propid=\"2\"");
        let rating = entry("99").replace(RATING_FMTID, "{64440492-4C8B-11D1-8B70-080036B11A03}");
        let xml = wrap(&format!("{other}{rating}"));
        assert_eq!(rating_from_xml(xml.as_bytes(), 27), Some(5));
        let renamed = xml
            .replace("r:", "other:")
            .replace("xmlns:r=", "xmlns:other=")
            .replace("<p:", "<property:")
            .replace("</p:", "</property:")
            .replace(" p:", " property:")
            .replace("xmlns:p=", "xmlns:property=");
        assert_eq!(rating_from_xml(renamed.as_bytes(), 27), Some(5));
        for (value, stars) in [
            (0, 0),
            (1, 1),
            (12, 1),
            (13, 2),
            (37, 2),
            (38, 3),
            (62, 3),
            (63, 4),
            (87, 4),
            (88, 5),
            (99, 5),
        ] {
            assert_eq!(document_rating(value), Some(stars));
        }
    }

    #[test]
    fn cdr_versions_14_and_15_use_summary_properties_with_text_values() {
        let xml = format!(
            r#"<s:Description xmlns:s="{SUMMARY_NS}"><s:Properties><s:Property array="false" fmtid="{{64440492-4C8B-11D1-8B70-080036B11A03}}" propid="2" type="uint">1</s:Property><s:Property array="false" fmtid="{{64440492-4C8B-11D1-8B70-080036B11A03}}" propid="9" type="uint"> 75 </s:Property></s:Properties></s:Description>"#
        );
        for version in [14, 15] {
            assert_eq!(rating_from_xml(xml.as_bytes(), version), Some(4));
            for invalid in [
                xml.replace(SUMMARY_NS, "https://example.invalid/summary"),
                xml.replace("array=\"false\"", "array=\"true\""),
                xml.replace("type=\"uint\"", "type=\"string\""),
                xml.replace(" 75 ", "-1"),
                xml.replace(" 75 ", "100"),
                xml.replace(" 75 ", "not a number"),
            ] {
                assert_eq!(rating_from_xml(invalid.as_bytes(), version), None);
            }
        }
    }

    #[test]
    fn cdr_invalid_rating_metadata_is_ignored() {
        let valid = wrap(&entry("75"));
        let invalid = [
            "not xml".to_string(),
            "<unclosed>".to_string(),
            valid.replace(PROPERTY_NS, "https://example.invalid/properties"),
            valid.replace(RDF_NS, "https://example.invalid/rdf"),
            valid.replace("p:array=\"false\"", "p:array=\"true\""),
            valid.replace("p:type=\"uint\"", "p:type=\"string\""),
            valid.replace(RATING_FMTID, "00000000-0000-0000-0000-000000000000"),
            valid.replace("p:propid=\"9\"", "p:propid=\"invalid\""),
            valid.replace("p:value=\"75\"", "p:value=\"-1\""),
            valid.replace("p:value=\"75\"", "p:value=\"100\""),
            valid.replace("p:value=\"75\"", "p:value=\"4294967296\""),
            valid.replace("p:value=\"75\"", "p:value=\"NaN\""),
            valid.replace("p:value=\"75\"", ""),
            valid.replace("p:value=\"75\"", "value=\"75\""),
            format!("<!DOCTYPE root [<!ENTITY value '75'>]>{valid}"),
        ];
        for xml in invalid {
            assert_eq!(rating_from_xml(xml.as_bytes(), 27), None, "{xml}");
        }
        assert_eq!(rating_from_xml(&[0xff, 0xfe], 27), None);
    }

    #[test]
    fn cdr_missing_damaged_and_oversized_summaries_keep_the_drawing_readable() {
        let oversized = format!(
            "{}{}",
            wrap(&entry("99")),
            " ".repeat(MAX_METADATA_BYTES as usize)
        );
        for version in [14, 15, 16, 27] {
            let bytes = crate::write::document_to_cdr_version(&Document::default(), version);
            for xml in [None, Some(&b"not xml"[..]), Some(oversized.as_bytes())] {
                let package = replace_metadata(&bytes, version, xml);
                let (back, _) = crate::open_bytes(&package, "Drawing").expect("drawing opens");
                assert_eq!(back.metadata.rating, 0);
                assert_eq!(back.pages.len(), 1);
            }
        }
    }
}
