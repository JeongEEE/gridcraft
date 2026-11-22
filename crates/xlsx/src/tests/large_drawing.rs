//! Large drawings are a sequence of bounded anchors, not one in-memory XML tree.

use std::sync::Arc;

use gridcraft_core::{CellRef, Value};

use super::{make_zip, minimal};
use crate::{read_xlsx, write_xlsx};

fn anchor(id: usize) -> String {
    // 28 elements per anchor, including harmless creation metadata as found in real files.
    format!(
        r#"<xdr:oneCellAnchor><xdr:from><xdr:col>23</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>28</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx="104775" cy="104775"/>
<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="{id}" name="Dot" descr="synthetic dot"><a:extLst><a:ext uri="synthetic"><a:creationId id="{id}"/></a:ext></a:extLst></xdr:cNvPr><xdr:cNvPicPr><a:picLocks noChangeAspect="1"/></xdr:cNvPicPr></xdr:nvPicPr>
<xdr:blipFill><a:blip r:embed="rId1"/><a:srcRect/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="104775" cy="104775"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></xdr:spPr></xdr:pic><xdr:clientData/></xdr:oneCellAnchor>"#
    )
}

fn package(anchors: &str) -> Vec<u8> {
    let drawing = format!(
        r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{anchors}</xdr:wsDr>"#
    );
    let bytes = minimal(
        r#"<sheetData><row r="1"><c r="A1"><v>7</v></c></row></sheetData><drawing r:id="draw"/>"#,
        &[
            (
                "xl/styles.xml",
                "<styleSheet><fonts><font/></fonts><fills><fill/></fills><borders><border/></borders><cellXfs><xf/></cellXfs></styleSheet>",
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships><Relationship Id="draw" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#,
            ),
            ("xl/drawings/drawing1.xml", &drawing),
            (
                "xl/drawings/_rels/drawing1.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.gif"/></Relationships>"#,
            ),
        ],
        r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
        "",
    );
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut data).unwrap();
        parts.push((f.name().to_owned(), data));
    }
    // Original synthetic bytes: the importer preserves media without decoding it.
    parts.push(("xl/media/image1.gif".into(), b"GIF89a synthetic dot".to_vec()));
    make_zip(&parts.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect::<Vec<_>>())
}

#[test]
fn large_drawing_survives_cell_edit_save_and_reopen() {
    // 37,000 × 28 = 1,036,000 XML elements: beyond the whole-tree limit, but each
    // independent picture is small. Never check in a user's workbook as a fixture.
    let anchors: String = (1..=37_000).map(anchor).collect();
    let (mut wb, report) = read_xlsx(&package(&anchors)).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let sheet = wb.sheet(0).unwrap();
    assert_eq!(sheet.images.len(), 37_000);
    let before = sheet.images.clone();
    wb.sheet_mut(0).unwrap().set_value(CellRef::new(0, 0), Value::Number(11.0));
    let saved = write_xlsx(&wb).unwrap();
    let (back, report) = read_xlsx(&saved).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let sheet = back.sheet(0).unwrap();
    assert_eq!(sheet.value(CellRef::new(0, 0)), Value::Number(11.0));
    assert_eq!(sheet.images, before, "every picture keeps its order, anchor, alt text and bytes");
}

#[test]
fn repeated_media_is_shared_without_merging_picture_objects() {
    let (wb, report) = read_xlsx(&package(&(anchor(1) + &anchor(2)))).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let images = &wb.sheet(0).unwrap().images;
    assert_eq!(images.len(), 2);
    assert_ne!(images[0].id, images[1].id);
    assert!(Arc::ptr_eq(&images[0].data, &images[1].data), "one package media part is inflated and retained once");
}

#[test]
fn broken_drawing_does_not_leave_partially_imported_pictures() {
    let (wb, report) = read_xlsx(&package(&(anchor(1) + "<broken attr=\""))).unwrap();
    assert!(wb.sheet(0).unwrap().images.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("skipped xl/drawings/drawing1.xml")));
}

#[test]
fn too_many_anchors_are_reported_without_retaining_a_partial_drawing() {
    let anchors = anchor(1) + &"<xdr:oneCellAnchor/>".repeat(100_000);
    let (wb, report) = read_xlsx(&package(&anchors)).unwrap();
    assert!(wb.sheet(0).unwrap().images.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("more than 100000 anchors")), "{:?}", report.warnings);
    assert_eq!(wb.sheet(0).unwrap().value(CellRef::new(0, 0)), Value::Number(7.0));
}

#[test]
fn streaming_does_not_remove_the_node_bound_inside_one_anchor() {
    let anchors = anchor(1) + "<xdr:oneCellAnchor><xdr:sp>" + &"<a:p/>".repeat(1_000_001) + "</xdr:sp></xdr:oneCellAnchor>";
    let (wb, report) = read_xlsx(&package(&anchors)).unwrap();
    assert!(wb.sheet(0).unwrap().images.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("more than 1000000 elements")), "{:?}", report.warnings);
}

#[test]
fn top_level_alternate_content_reads_its_first_choice_once() {
    // Streaming splits only bare anchors; a wrapped one must still be read, from one branch.
    let wrapped = format!(
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="a14">{}</mc:Choice><mc:Fallback>{}{}</mc:Fallback></mc:AlternateContent>"#,
        anchor(3),
        anchor(4),
        anchor(5)
    );
    let (wb, _) = read_xlsx(&package(&format!("{}{wrapped}", anchor(2)))).unwrap();
    let images = &wb.sheet(0).unwrap().images;
    assert_eq!(images.len(), 2, "the bare anchor and the choice's anchor, not the fallback's");
}
