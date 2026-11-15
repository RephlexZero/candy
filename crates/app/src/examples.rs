pub struct Example {
    pub title: &'static str,
    pub file: &'static str,
    pub blurb: &'static str,
    pub tags: &'static [&'static str],
    pub text: &'static str,
}

pub const EXAMPLES: &[Example] = &[
    Example {
        title: "Motorsport ECU",
        file: "motorsport_ecu.dbc",
        blurb: "Engine, PDM, dash and logger at 1 Mbit/s. Multiplexed PDM telemetry, signal groups, motorsport naming.",
        tags: &["1 Mbit/s", "Mux", "Groups"],
        text: include_str!("../../../fixtures/motorsport_ecu.dbc"),
    },
    Example {
        title: "EV battery pack",
        file: "ev_bms.dbc",
        blurb: "BMS, VCU, inverter and a CAN FD charger. Nested + extended multiplexing, floats, Motorola byte order.",
        tags: &["CAN FD", "Nested mux", "Float"],
        text: include_str!("../../../fixtures/ev_bms.dbc"),
    },
    Example {
        title: "J1939 engine",
        file: "j1939_engine.dbc",
        blurb: "Heavy-duty SAE J1939 parameter groups (EEC1, ET1, CCVS1…) with 29-bit ids, PGNs and SPNs.",
        tags: &["29-bit", "J1939", "250 kbit/s"],
        text: include_str!("../../../fixtures/j1939_engine.dbc"),
    },
];
