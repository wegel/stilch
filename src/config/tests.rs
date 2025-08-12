use super::*;
use crate::config::parser::parse_config;

    #[test]
    fn test_parse_input_config_keyboard() {
        let config_str = r#"input type:keyboard { repeat_delay 200 repeat_rate 42 xkb_layout us xkb_variant altgr-intl }"#;
        
        let config = parse_config(config_str).unwrap();
        assert_eq!(config.input_configs.len(), 1);
        
        let input = &config.input_configs[0];
        assert_eq!(input.identifier, "type:keyboard");
        assert_eq!(input.repeat_delay, Some(200));
        assert_eq!(input.repeat_rate, Some(42));
        assert_eq!(input.xkb_layout.as_deref(), Some("us"));
        assert_eq!(input.xkb_variant.as_deref(), Some("altgr-intl"));
    }

    #[test]
    fn test_parse_input_config_touchpad() {
        let config_str = r#"input type:touchpad { tap enabled natural_scroll disabled accel_speed 0.5 scroll_method two_finger }"#;
        
        let config = parse_config(config_str).unwrap();
        assert_eq!(config.input_configs.len(), 1);
        
        let input = &config.input_configs[0];
        assert_eq!(input.identifier, "type:touchpad");
        assert_eq!(input.tap, Some(true));
        assert_eq!(input.natural_scroll, Some(false));
        assert_eq!(input.accel_speed, Some(0.5));
        assert!(matches!(input.scroll_method, Some(ScrollMethod::TwoFinger)));
    }

    #[test]
    fn test_parse_input_config_pointer() {
        let config_str = r#"input type:pointer { accel_profile flat left_handed yes middle_emulation enabled }"#;
        
        let config = parse_config(config_str).unwrap();
        assert_eq!(config.input_configs.len(), 1);
        
        let input = &config.input_configs[0];
        assert_eq!(input.identifier, "type:pointer");
        assert!(matches!(input.accel_profile, Some(AccelProfile::Flat)));
        assert_eq!(input.left_handed, Some(true));
        assert_eq!(input.middle_emulation, Some(true));
    }