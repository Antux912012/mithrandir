use mithrandir::hardware::{
    battery::{extract_fru_part, get_compatible_frus, BatteryInfo},
    display::{parse_edid_bytes, DisplayInfo},
    memory::{deduce_memory_specs, parse_meminfo, MemoryInfo},
    network::{identify_pci_network_device, NetworkCardInfo},
    storage::DiskInfo,
    system::{decode_chassis_type, SystemInfo},
};

#[test]
fn test_system_info_collection_and_chassis_decoding() {
    assert_eq!(decode_chassis_type("3"), "Desktop");
    assert_eq!(decode_chassis_type("9"), "Laptop");
    assert_eq!(decode_chassis_type("10"), "Notebook");
    assert_eq!(decode_chassis_type("31"), "Convertible 2-in-1");
    assert_eq!(decode_chassis_type("999"), "Personal Computer");

    let sys = SystemInfo::collect();
    assert!(!sys.vendor.is_empty());
    assert!(!sys.cpu_model.is_empty());
    assert!(sys.cpu_cores >= 1);
    assert!(sys.cpu_threads >= 1);

    let dev_name = sys.full_device_name();
    assert!(!dev_name.is_empty());

    let query = sys.replacement_search_query();
    assert!(query.contains("parts replacement"));
}

#[test]
fn test_battery_fru_and_compatibility() {
    assert_eq!(extract_fru_part("LNV-01AV436"), "01AV436");
    assert_eq!(extract_fru_part("01AV435"), "01AV435");
    assert_eq!(extract_fru_part("  PREFIX-MODEL  "), "MODEL");

    let comp = get_compatible_frus("01AV436");
    assert!(!comp.is_empty());
    assert!(comp.iter().any(|c| c.contains("01AV435")));

    let comp_empty = get_compatible_frus("UNKNOWN_FRU_123");
    assert!(comp_empty.is_empty());

    let bats = BatteryInfo::collect_all("Test Device");
    // Should run gracefully without crashing whether running in container, VM, or bare metal
    for b in bats {
        assert!(!b.name.is_empty());
        assert!(b.percentage <= 100);
    }
}

#[test]
fn test_memory_spec_deduction_and_collection() {
    let (d5_type, d5_speed, d5_pin, _) = deduce_memory_specs("13th Gen Intel(R) Core(TM) i7-13700H");
    assert_eq!(d5_type, "DDR5");
    assert!(d5_speed.contains("4800"));
    assert!(d5_pin.contains("262-pin"));

    let (d4_type, _, d4_pin, _) = deduce_memory_specs("Intel(R) Core(TM) i7-8565U CPU @ 1.80GHz");
    assert_eq!(d4_type, "DDR4");
    assert!(d4_pin.contains("260-pin"));

    let (total_kb, avail_kb) = parse_meminfo();
    if total_kb > 0 {
        assert!(avail_kb <= total_kb);
    }

    let mem = MemoryInfo::collect("Test ThinkPad", "Intel i7-8565U");
    assert!(mem.recommended_search.contains("RAM upgrade"));
}

#[test]
fn test_storage_collection() {
    let disks = DiskInfo::collect_all("Test Device");
    for d in disks {
        assert!(!d.name.is_empty());
        assert!(d.capacity_gb > 0.0);
        assert!(!d.form_factor.is_empty());
        assert!(!d.upgrade_recommendation.is_empty());
    }
}

#[test]
fn test_display_edid_parsing_and_collection() {
    // Construct mock 128-byte EDID block
    let mut mock_edid = vec![0u8; 128];
    // Header
    mock_edid[0..8].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]);
    // Manufacturer ID for "BOE":
    // B = 2, O = 15, E = 5
    // raw = (2 << 10) | (15 << 5) | 5 = 2048 | 480 | 5 = 2533 = 0x09e5
    mock_edid[8] = 0x09;
    mock_edid[9] = 0xe5;

    // Descriptor at offset 54: text descriptor (type 0xfe)
    mock_edid[54] = 0x00;
    mock_edid[55] = 0x00;
    mock_edid[56] = 0x00;
    mock_edid[57] = 0xfe;
    let model_bytes = b"NV140FHM-N49\n";
    mock_edid[59..59 + model_bytes.len()].copy_from_slice(model_bytes);

    let (mfg, panel) = parse_edid_bytes(&mock_edid);
    assert_eq!(mfg, "BOE Technology");
    assert_eq!(panel, "NV140FHM-N49");

    let displays = DisplayInfo::collect_all("Test Device");
    for disp in displays {
        assert!(!disp.connector.is_empty());
        assert!(!disp.resolution.is_empty());
    }
}

#[test]
fn test_network_pci_identification_and_collection() {
    assert_eq!(
        identify_pci_network_device("0x8086", "0x2723"),
        Some("Intel Wi-Fi 6 AX200".into())
    );
    assert_eq!(
        identify_pci_network_device("0x8086", "0x2725"),
        Some("Intel Wi-Fi 6E AX210".into())
    );
    assert_eq!(
        identify_pci_network_device("0x8086", "0x24fd"),
        Some("Intel Dual Band Wireless-AC 8265".into())
    );
    assert_eq!(
        identify_pci_network_device("0x10ec", "0xc822"),
        Some("Realtek Wireless Controller (0xc822)".into())
    );
    assert_eq!(
        identify_pci_network_device("0x14e4", "0x43a0"),
        Some("Broadcom Wireless Controller (0x43a0)".into())
    );
    assert_eq!(
        identify_pci_network_device("0x17cb", "0x1103"),
        Some("Qualcomm Atheros Wireless (0x1103)".into())
    );

    let adapters = NetworkCardInfo::collect_all("Test Device");
    for adapter in adapters {
        assert!(!adapter.interface_name.is_empty());
        assert!(!adapter.chipset_name.is_empty());
    }
}
