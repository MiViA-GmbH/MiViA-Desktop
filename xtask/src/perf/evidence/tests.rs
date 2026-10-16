use super::{AMD64, I386, absent, canary_hits, decode, job_ids, missing_ops, pe_machine};

fn pe(machine: u16) -> Vec<u8> {
    let mut bytes = vec![0_u8; 0x90];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    bytes[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
    bytes
}

#[test]
fn the_machine_comes_from_the_pe_header() {
    assert_eq!(pe_machine(&pe(I386)), Some(I386));
    assert_eq!(pe_machine(&pe(AMD64)), Some(AMD64));
    assert_eq!(pe_machine(b"MZ"), None);
    let mut broken = pe(I386);
    broken[0x80] = b'X';
    assert_eq!(pe_machine(&broken), None);
}

#[test]
fn utf16_and_utf8_dumps_read_the_same() {
    let text = "Image \\smoke\\Downloads\\x.exe";
    let mut wide = vec![0xff, 0xfe];
    wide.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(decode(&wide), text);
    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend(text.as_bytes());
    assert_eq!(decode(&bom), text);
    assert_eq!(decode(text.as_bytes()), text);
}

#[test]
fn only_a_canary_in_a_planted_folder_counts() {
    let trace = r"<Data Name='ImageName'>\Device\HarddiskVolume3\a\smoke\Downloads\VERSION.dll</Data>
<Data Name='ImageName'>\Device\HarddiskVolume3\Windows\System32\uxtheme.dll</Data>
<Data Name='ImageName'>\Device\HarddiskVolume3\a\smoke\install\winmm.dll.bak</Data>
<Data Name='ImageName'>\Device\HarddiskVolume3\a\smoke\install\cryptbase.dll</Data>";
    assert_eq!(
        canary_hits(trace, &["downloads", "install"]),
        vec![r"\downloads\version.dll".to_owned()]
    );
    assert_eq!(canary_hits("", &["downloads"]), Vec::<String>::new());
}

#[test]
fn absent_needles_are_listed_without_case() {
    assert_eq!(
        absent(r"\x\MIVIA-DESKTOP.exe", &["mivia-desktop.exe", "un_a.exe"]),
        vec!["un_a.exe"]
    );
}

#[test]
fn job_ids_come_from_the_listing() {
    assert_eq!(
        job_ids(r#"{"data":[{"id":"a"},{"id":"b"},{"x":1}]}"#),
        vec!["a", "b"]
    );
    assert_eq!(job_ids("not json"), Vec::<String>::new());
}

#[test]
fn missing_ops_name_what_the_journal_lacks() {
    let journal = r#"[{"op":"Upload","at":1},{"op":"Submit","at":2}]"#;
    assert_eq!(
        missing_ops(journal, &["Upload", "Submit", "Status"]),
        vec!["Status"]
    );
    assert_eq!(missing_ops("[]", &["Upload"]), vec!["Upload"]);
}
