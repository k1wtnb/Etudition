//! Etudition のアプリ（Tauri の殻）。配線だけを置く。

// リリースのビルドで、Windows のコンソールの窓を出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("Tauri のアプリを起動できなかった");
}

#[cfg(test)]
mod boundary_probe {
    //! P0 の④：境界の時刻の形を決めるための確かめ（計画 2-3 の2）。

    use serde::Serialize;
    use ts_rs::TS;

    #[derive(Serialize, TS)]
    struct TimeProbe {
        ns: i64,
    }

    #[test]
    fn ts_rs_writes_i64_as_bigint() {
        let decl = TimeProbe::decl(&ts_rs::Config::default());
        assert!(decl.contains("ns: bigint"), "{decl}");
    }

    #[test]
    fn serde_json_writes_i64_as_plain_number() {
        // 2^53 + 1 は JavaScript の number では正確に表せない
        let json = serde_json::to_string(&TimeProbe {
            ns: (1_i64 << 53) + 1,
        })
        .unwrap();
        assert_eq!(json, r#"{"ns":9007199254740993}"#);
    }
}
