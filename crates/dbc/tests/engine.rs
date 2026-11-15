use candy_dbc::decode::{self, decode_message, encode_physical, parse_line};
use candy_dbc::layout::{self, signal_bits};
use candy_dbc::lint::{self, Fix, Target};
use candy_dbc::naming::{self, Case};
use candy_dbc::*;

const FIXTURES: &[(&str, &str)] = &[
    ("motorsport_ecu", include_str!("../../../fixtures/motorsport_ecu.dbc")),
    ("ev_bms", include_str!("../../../fixtures/ev_bms.dbc")),
    ("j1939_engine", include_str!("../../../fixtures/j1939_engine.dbc")),
];

fn load(name: &str) -> Dbc {
    let src = FIXTURES.iter().find(|f| f.0 == name).unwrap().1;
    let out = parse(src);
    assert!(out.errors.is_empty(), "{name}: {:?}", out.errors);
    out.dbc
}

#[test]
fn fixtures_parse_cleanly_and_round_trip() {
    for (name, src) in FIXTURES {
        let out = parse(src);
        assert!(out.errors.is_empty(), "{name}: {:?}", out.errors);
        let written = write(&out.dbc);
        let again = parse(&written);
        assert!(again.errors.is_empty(), "{name} re-parse: {:?}\n{written}", again.errors);
        assert_eq!(out.dbc, again.dbc, "{name} changed after a write/parse cycle");
        // Writing is deterministic.
        assert_eq!(written, write(&again.dbc));
    }
}

#[test]
fn model_details() {
    let d = load("ev_bms");
    let cells = d.message(272).unwrap();
    assert_eq!(cells.multiplexor().unwrap().name, "CellGroup");
    assert_eq!(cells.signal("Cell05_Voltage").unwrap().mux, Mux::Multiplexed(1));
    let diag = d.message(288).unwrap();
    assert_eq!(diag.signal("DiagSubPage").unwrap().mux, Mux::MultiplexedMultiplexor(1));
    assert_eq!(diag.signal("FaultCode").unwrap().ext_mux[0].switch, "DiagSubPage");
    assert_eq!(d.message(512).unwrap().signal("MotorTorque").unwrap().value_type, ValueType::Float32);
    assert_eq!(d.message(769).unwrap().extra_transmitters, vec!["VCU", "BMS"]);
    assert_eq!(d.cycle_time_ms(cells), Some(50.0));
    let chg = d.message(768).unwrap();
    assert_eq!(d.enum_label("VFrameFormat", chg.attribute("VFrameFormat").unwrap()).as_deref(), Some("StandardCAN_FD"));

    let j = load("j1939_engine");
    let eec1 = j.message_by_name("EEC1").unwrap();
    assert!(eec1.is_extended());
    assert_eq!(eec1.can_id(), 0x0CF00400);
    let id = eec1.j1939().unwrap();
    assert_eq!((id.priority, id.pgn, id.source), (3, 61444, 0));
}

#[test]
fn intel_and_motorola_bits() {
    let mut s = Signal { start_bit: 4, size: 8, ..Default::default() };
    assert_eq!(signal_bits(&s), (4..12).collect::<Vec<_>>());
    s.byte_order = ByteOrder::BigEndian;
    s.start_bit = 7;
    s.size = 16;
    // MSB at byte0 bit7, LSB at byte1 bit0.
    let bits = signal_bits(&s);
    assert_eq!(bits.first(), Some(&8));
    assert_eq!(bits.last(), Some(&7));
    assert_eq!(layout::motorola_start_from_lsb(8, 16), 7);
    assert_eq!(layout::bytes_needed(&s), 2);
}

#[test]
fn decode_known_frames() {
    let d = load("motorsport_ecu");
    let eng = d.message(1600).unwrap();
    // 6500 rpm, 45.6 %, 101.3 kPa, lambda 0.998
    let data = [0x64, 0x19, 0xC8, 0x01, 0xF5, 0x03, 0xE6, 0x03];
    let v = decode_message(eng, &data);
    assert_eq!(v[0].physical, 6500.0);
    assert!((v[1].physical - 45.6).abs() < 1e-9);
    assert!((v[2].physical - 101.3).abs() < 1e-9);
    assert!((v[3].physical - 0.998).abs() < 1e-9);

    // Signed.
    let damp = d.message(1617).unwrap();
    let v = decode_message(damp, &[0x18, 0xFC, 0, 0, 0, 0, 0, 0]);
    assert!((v[0].physical - -10.0).abs() < 1e-9);

    // Simple mux: page 1 → cooling outputs active, page-0 signals inactive.
    let pdm = d.message(1664).unwrap();
    let v = decode_message(pdm, &[0x01, 0x32, 0, 0, 0, 0, 0, 0]);
    let get = |n: &str| v.iter().find(|x| x.name == n).unwrap();
    assert!(get("Out_Curr_Fuel_Pump").active);
    assert!((get("Out_Curr_Fuel_Pump").physical - 10.0).abs() < 1e-9);
    assert!(!get("PDM_Volt_In").active);
    assert_eq!(get("PDM_Page").label.as_deref(), Some("Cooling"));

    // Motorola + nested/extended mux + float.
    let e = load("ev_bms");
    let chg = e.message(768).unwrap();
    let mut buf = vec![0u8; 12];
    encode_physical(chg.signal("ChargerOutputVoltage").unwrap(), &mut buf, 400.0);
    assert_eq!(&buf[0..2], &[0x0F, 0xA0]);
    encode_physical(chg.signal("ChargerEfficiency").unwrap(), &mut buf, 0.95);
    let v = decode_message(chg, &buf);
    assert!((v[0].physical - 400.0).abs() < 1e-9);
    assert!((v[4].physical - 0.95).abs() < 1e-6);

    let diag = e.message(288).unwrap();
    let v = decode_message(diag, &[1, 3, 0x34, 0x12, 0, 0, 0, 0]);
    let get = |n: &str| v.iter().find(|x| x.name == n).unwrap().clone();
    assert!(get("FaultCode").active);
    assert_eq!(get("FaultCode").physical, 0x1234 as f64);
    assert!(!get("IsoResistance").active);
    let v = decode_message(diag, &[1, 99, 0, 0, 0, 0, 0, 0]);
    assert!(!v.iter().find(|x| x.name == "FaultCode").unwrap().active);
}

#[test]
fn encode_decode_round_trip_every_signal() {
    for (name, _) in FIXTURES {
        let d = load(name);
        for m in &d.messages {
            for s in &m.signals {
                let mut buf = vec![0u8; m.size as usize];
                let target = if s.max > s.min { (s.min + s.max) / 2.0 } else { s.offset };
                encode_physical(s, &mut buf, target);
                let raw = decode::extract_raw(s, &buf);
                let back = s.to_physical(decode::raw_to_value(s, raw));
                let tol = if s.value_type.is_float() { 1e-3 * target.abs().max(1.0) } else { s.factor.abs() };
                assert!((back - target).abs() <= tol, "{name}/{}.{}: {target} → {back}", m.name, s.name);
            }
        }
    }
}

#[test]
fn log_formats() {
    let f = parse_line("(1699999999.123456) can0 1A0#DEADBEEF").unwrap();
    assert_eq!((f.id, f.extended, f.data.clone()), (0x1A0, false, vec![0xDE, 0xAD, 0xBE, 0xEF]));
    assert_eq!(f.timestamp, Some(1699999999.123456));
    let f = parse_line("  can0  18FEF100   [8]  00 01 02 03 04 05 06 07").unwrap();
    assert!(f.extended);
    assert_eq!(f.id, 0x18FEF100);
    assert_eq!(f.data.len(), 8);
    let f = parse_line("   0.012345 1  18FEF100x       Rx   d 8 00 01 02 03 04 05 06 07").unwrap();
    assert!(f.extended);
    assert_eq!(f.data[7], 7);
    let f = parse_line("    1)       12.3  Rx         01A0  4  DE AD BE EF").unwrap();
    assert_eq!((f.id, f.extended), (0x1A0, false));
    let f = parse_line("0x640 64 19 C8 01 F5 03 E6 03").unwrap();
    assert_eq!(f.id, 0x640);
    assert_eq!(f.data[0], 0x64);
    let f = parse_line("640: 6419C801F503E603").unwrap();
    assert_eq!(f.data.len(), 8);
    assert!(parse_line("// comment").is_none());

    let j = load("j1939_engine");
    // EEC1 from source address 0x17 still matches the DBC entry for SA 0x00.
    let f = parse_line("0CF00417#FF7D7D401F00FFFF").unwrap();
    assert_eq!(decode::find_message(&j, &f).unwrap().name, "EEC1");
}

#[test]
fn error_tolerance() {
    let src = "VERSION \"\"\nBU_: A\nBO_ 1 Ok: 8 A\n SG_ Good : 0|8@1+ (1,0) [0|255] \"\" A\n SG_ Bad : x|8@1+ (1,0) [0|1] \"\" A\nBO_ 2 Next: 8 A\n SG_ Fine : 0|8@1+ (1,0) [0|255] \"\" A\nGARBAGE_ here;\nCM_ BO_ 2 \"still parsed\";\n";
    let out = parse(src);
    assert_eq!(out.errors.len(), 1, "{:?}", out.errors);
    assert_eq!(out.errors[0].line, 5);
    assert_eq!(out.dbc.messages.len(), 2);
    assert_eq!(out.dbc.message(2).unwrap().comment.as_deref(), Some("still parsed"));
    assert_eq!(out.dbc.extra, vec!["GARBAGE_ here;"]);
}

#[test]
fn escaped_strings_round_trip() {
    let mut d = Dbc::new();
    d.comment = Some("He said \"hi\" \\ bye".into());
    let again = parse_strict(&write(&d)).unwrap();
    assert_eq!(again.comment, d.comment);
}

#[test]
fn lint_finds_and_fixes() {
    let src = "VERSION \"\"\nBU_: A B\nBO_ 1 M1: 2 A\n SG_ S1 : 0|8@1+ (1,0) [0|255] \"\" C\n SG_ S2 : 4|8@1+ (0,0) [0|0] \"\" A\n SG_ S3 : 12|8@1+ (1,0) [10|0] \"\" A\nBO_ 1 M2: 9 A\n";
    let mut d = parse(src).dbc;
    let issues = lint::lint(&d, None);
    let rules: Vec<&str> = issues.iter().map(|i| i.rule).collect();
    for r in ["duplicate-id", "signal-overlap", "zero-factor", "range-inverted", "signal-outside-payload", "payload-size", "unknown-node", "unused-node"] {
        assert!(rules.contains(&r), "missing {r} in {rules:?}");
    }
    let n = lint::fix_all(&mut d, &issues, None);
    assert!(n >= 5);
    assert!(d.node("C").is_some());
    assert_eq!(d.message(1).unwrap().signal("S2").unwrap().factor, 1.0);
}

#[test]
fn clean_fixtures_have_no_errors() {
    for (name, _) in FIXTURES {
        let d = load(name);
        let conv = naming::from_dbc(&d);
        let issues = lint::lint(&d, conv.as_ref());
        let errors: Vec<_> = issues.iter().filter(|i| i.severity == lint::Severity::Error).collect();
        assert!(errors.is_empty(), "{name}: {errors:#?}");
    }
}

#[test]
fn naming_conventions() {
    assert_eq!(naming::split_words("ABSWheelSpeed_FL2"), vec!["ABS", "Wheel", "Speed", "FL2"]);
    assert_eq!(naming::to_case("engine_speed", Case::Pascal), "EngineSpeed");
    assert_eq!(naming::to_case("EngineSpeed", Case::Snake), "engine_speed");
    assert_eq!(naming::to_case("ENGINE_SPEED", Case::Camel), "engineSpeed");
    assert_eq!(naming::to_case("WhlSpeedFL", Case::PascalSnake), "Whl_Speed_FL");
    assert_eq!(naming::to_case("ABSStatus", Case::ScreamingSnake), "ABS_STATUS");
    assert!(Case::PascalSnake.matches("Eng_Data_1"));
    assert!(!Case::PascalSnake.matches("Eng_data"));
    assert_eq!(naming::fix_corners("Tyre_Temp_LeftFront"), "Tyre_Temp_FL");
    assert_eq!(naming::fix_corners("WheelSpeedFrontRight"), "WheelSpeedFR");
    assert_eq!(naming::shorten("Battery_Temperature_Maximum_Value_Front", 32, Case::PascalSnake).len() <= 32, true);

    let d = load("motorsport_ecu");
    let conv = naming::from_dbc(&d).unwrap();
    assert_eq!(conv.id, "motorsport");
    let issues = naming::check_dbc(&d, &conv);
    assert_eq!(issues.len(), 1, "{issues:#?}");
    assert_eq!(issues[0].suggestion, "Tyre_Temp_FL");

    // Applying a rename fix cascades.
    let mut d = d;
    lint::apply_fix(&mut d, &Fix::Rename { target: Target::Signal(1618, "Tyre_Temp_LeftFront".into()), new_name: "Tyre_Temp_FL".into() });
    assert!(naming::check_dbc(&d, &conv).is_empty());

    // Snake preset on the J1939 file proposes valid, unique names.
    let j = load("j1939_engine");
    let snake = naming::preset("snake").unwrap();
    let issues = naming::check_dbc(&j, &snake);
    assert!(issues.iter().any(|i| i.suggestion == "eng_speed"));
    for i in &issues {
        assert!(naming::is_identifier(&i.suggestion), "{}", i.suggestion);
        assert!(i.suggestion.len() <= 32, "{}", i.suggestion);
    }
    // Custom convention survives being stored in the file.
    let mut custom = snake.clone();
    custom.id = "custom".into();
    custom.signal.max_len = Some(20);
    let mut j2 = j.clone();
    set_attribute(&mut j2.attributes, naming::CONVENTION_ATTR, Some(AttrValue::Str(naming::to_attr_string(&custom))));
    let j3 = parse_strict(&write(&j2)).unwrap();
    assert_eq!(naming::from_dbc(&j3), Some(custom));
}

#[test]
fn diff_and_merge() {
    let a = load("ev_bms");
    let mut b = a.clone();
    b.message_mut(256).unwrap().signal_mut("PackVoltage").unwrap().factor = 0.02;
    b.remove_message(512);
    b.nodes.push(Node { name: "DCDC".into(), ..Default::default() });
    let mut new_msg = Message { id: 0x400, name: "DCDC_Status".into(), ..Default::default() };
    new_msg.signals.push(Signal { name: "OutV".into(), ..Default::default() });
    b.messages.push(new_msg);

    let changes = diff::diff(&a, &b);
    assert_eq!(changes.len(), 4, "{changes:#?}");
    let merged = diff::merge(&a, &b, &(0..changes.len()).collect::<Vec<_>>());
    assert_eq!(diff::diff(&merged, &b), vec![]);
    let partial = diff::merge(&a, &b, &[0]);
    assert_eq!(diff::diff(&partial, &b).len(), 3);
}

#[test]
fn free_bits_and_utilisation() {
    let d = load("motorsport_ecu");
    let gb = d.message(1648).unwrap();
    assert_eq!(layout::find_free_bits(gb, 4, None), Some(25));
    let pdm = d.message(1664).unwrap();
    assert_eq!(layout::mux_values(pdm), vec![0, 1, 2, 3]);
    assert!(layout::overlaps(pdm).is_empty());
    assert!(layout::utilisation(d.message(1600).unwrap()) == 1.0);
}

#[test]
fn bus_load() {
    let d = load("motorsport_ecu");
    let s = stats::summary(&d, stats::bitrate(&d));
    assert_eq!(s.messages, 9);
    assert!(s.bus_load > 0.05 && s.bus_load < 0.5, "{}", s.bus_load);
}

mod props {
    use super::*;
    use proptest::prelude::*;

    fn ident() -> impl Strategy<Value = String> {
        "[A-Za-z][A-Za-z0-9_]{0,20}"
    }

    proptest! {
        #[test]
        fn writer_round_trips_random_messages(
            names in proptest::collection::hash_set(ident(), 1..6),
            start in 0u32..56, size in 1u32..9, factor in 0.001f64..1000.0, offset in -1000f64..1000.0,
            big in any::<bool>(), signed in any::<bool>(), comment in "[ -~]{0,30}",
        ) {
            let mut d = Dbc::new();
            d.nodes.push(Node { name: "ECU".into(), ..Default::default() });
            let mut m = Message { id: 0x123, name: "Msg".into(), transmitter: "ECU".into(), ..Default::default() };
            for n in &names {
                m.signals.push(Signal {
                    name: n.clone(), start_bit: start, size,
                    byte_order: if big { ByteOrder::BigEndian } else { ByteOrder::LittleEndian },
                    value_type: if signed { ValueType::Signed } else { ValueType::Unsigned },
                    factor, offset, min: offset, max: offset + factor,
                    comment: Some(comment.clone()),
                    ..Default::default()
                });
            }
            d.messages.push(m);
            let back = parse_strict(&write(&d)).unwrap();
            prop_assert_eq!(back, d);
        }

        #[test]
        fn encode_decode_raw(start in 0u32..48, size in 1u32..17, raw in any::<u64>(), big in any::<bool>()) {
            let s = Signal {
                start_bit: if big { start | 7 } else { start }, size,
                byte_order: if big { ByteOrder::BigEndian } else { ByteOrder::LittleEndian },
                ..Default::default()
            };
            let raw = raw & ((1u64 << size) - 1);
            let mut buf = [0u8; 8];
            decode::insert_raw(&s, &mut buf, raw);
            prop_assert_eq!(decode::extract_raw(&s, &buf), raw);
        }
    }
}
