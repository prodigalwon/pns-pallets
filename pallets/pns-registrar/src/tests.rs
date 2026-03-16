use crate::*;
use codec::Encode;
use polkadot_sdk::frame_support::{assert_noop, assert_ok};
use mock::*;
use pns_resolvers::resolvers::TextKind;
use polkadot_sdk::sp_runtime::testing::TestSignature;
use traits::Label;

const DAYS: u64 = 24 * 60 * 60;

#[test]
fn register_test() {
    new_test_ext().execute_with(|| {
        // now not supported chinese domain name
        let name = "中文测试".as_bytes();
        assert_noop!(
            Registrar::register(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                name.to_vec(),
                RICH_ACCOUNT,
            ),
            registrar::Error::<Test>::ParseLabelFailed
        );

        // label length too short
        assert_noop!(
            Registrar::register(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"hello".to_vec(),
                RICH_ACCOUNT,
            ),
            registrar::Error::<Test>::LabelInvalid
        );

        let name = b"hello-world";

        let name2 = b"world-hello";

        use traits::PriceOracle as _;

        let total_price = PriceOracle::registration_fee(name.len()).unwrap();
        let init_free = Balances::free_balance(RICH_ACCOUNT);
        // a right call
        assert_ok!(Registrar::register(
            RuntimeOrigin::signed(RICH_ACCOUNT),
            name.to_vec(),
            RICH_ACCOUNT,
        ));

        let now_free = Balances::free_balance(RICH_ACCOUNT);
        assert_eq!(init_free - now_free, total_price);

        let (label, len) = Label::new_with_len(name).unwrap();

        let (label2, len2) = Label::new_with_len(name2).unwrap();

        assert!(len == 11);

        assert!(len2 == 11);
        let node = label.encode_with_node(&DOT_BASENODE);
        let node2 = label2.encode_with_node(&DOT_BASENODE);

        let info = registrar::RegistrarInfos::<Test>::get(node).unwrap();

        let now = Timestamp::now();

        assert_eq!(info.expire, now + MaxRegistrationDuration::get());

        assert_noop!(
            Registrar::register(
                RuntimeOrigin::signed(MONEY_ACCOUNT),
                name.to_vec(),
                MONEY_ACCOUNT,
            ),
            registrar::Error::<Test>::Occupied
        );

        assert_noop!(
            Registrar::register(
                RuntimeOrigin::signed(POOR_ACCOUNT),
                name2.to_vec(),
                POOR_ACCOUNT,
            ),
            polkadot_sdk::pallet_balances::Error::<Test>::InsufficientBalance
        );
        let price_free = PriceOracle::registration_fee(name2.len()).unwrap();

        // Balance equal to fee is not enough due to existential deposit requirement.
        Balances::set_balance(RuntimeOrigin::root(), POOR_ACCOUNT, price_free, 0).unwrap();

        assert_noop!(
            Registrar::register(
                RuntimeOrigin::signed(POOR_ACCOUNT),
                name2.to_vec(),
                POOR_ACCOUNT,
            ),
            polkadot_sdk::pallet_balances::Error::<Test>::InsufficientBalance
        );

        Balances::set_balance(RuntimeOrigin::root(), POOR_ACCOUNT, price_free * 2, 0).unwrap();

        assert_ok!(Registrar::register(
            RuntimeOrigin::signed(POOR_ACCOUNT),
            name2.to_vec(),
            POOR_ACCOUNT,
        ));

        assert_ok!(Registrar::renew(
            RuntimeOrigin::signed(RICH_ACCOUNT),
        ));

        let info = registrar::RegistrarInfos::<Test>::get(node).unwrap();

        assert_noop!(
            Registrar::transfer(RuntimeOrigin::signed(MONEY_ACCOUNT), RICH_ACCOUNT),
            registrar::Error::<Test>::NoCanonicalName
        );

        // Renewal always resets to now + MaxRegistrationDuration (now == 0 in tests)
        assert_eq!(info.expire, MaxRegistrationDuration::get());

        // MONEY_ACCOUNT has no canonical name — cannot renew someone else's name.
        assert_noop!(
            Registrar::renew(RuntimeOrigin::signed(MONEY_ACCOUNT)),
            registrar::Error::<Test>::NoCanonicalName
        );

        assert!(Nft::is_owner(&RICH_ACCOUNT, (0, node)));

        assert_ok!(Registrar::transfer(
            RuntimeOrigin::signed(RICH_ACCOUNT),
            MONEY_ACCOUNT,
        ));

        assert!(Nft::is_owner(&MONEY_ACCOUNT, (0, node)));

        assert_ok!(Registrar::transfer(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            RICH_ACCOUNT,
        ));

        assert!(Nft::is_owner(&RICH_ACCOUNT, (0, node)));

        assert_ok!(Registrar::mint_subname(
            RuntimeOrigin::signed(RICH_ACCOUNT),
            node,
            b"test".to_vec(),
            MONEY_ACCOUNT
        ));

        assert_noop!(
            Registrar::mint_subname(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                node,
                b"test".to_vec(),
                MONEY_ACCOUNT
            ),
            registrar::Error::<Test>::NotExistOrOccupied
        );

        assert_ok!(Registrar::mint_subname(
            RuntimeOrigin::signed(RICH_ACCOUNT),
            node,
            b"test1".to_vec(),
            MONEY_ACCOUNT
        ));

        assert!(Nft::is_owner(&POOR_ACCOUNT, (0, node2)));

        assert_ok!(Registrar::mint_subname(
            RuntimeOrigin::signed(POOR_ACCOUNT),
            node2,
            b"test1".to_vec(),
            MONEY_ACCOUNT
        ));

        assert_noop!(
            Registrar::mint_subname(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                node2,
                b"test2".to_vec(),
                MONEY_ACCOUNT
            ),
            registry::Error::<Test>::NoPermission
        );

        let (test_label, _) = Label::new_with_len(b"test1").unwrap();
        let test_node = test_label.encode_with_node(&node2);

        assert!(Nft::is_owner(&MONEY_ACCOUNT, (0, test_node)));
    });
}

#[test]
fn redeem_code_test() {
    new_test_ext().execute_with(|| {
        assert_ok!(RedeemCode::mint_redeem(
            RuntimeOrigin::signed(MANAGER_ACCOUNT),
            0,
            10
        ));

        let nouce = 0_u32;
        let (label, _) = Label::new_with_len("cupnfish".as_bytes()).unwrap();
        let label_node = label.node;
        let duration = MinRegistrationDuration::get();

        let signature = (label_node, duration, nouce).encode();

        println!("{:?}", signature);

        assert_noop!(
            RedeemCode::name_redeem(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"cupnfish".to_vec(),
                MinRegistrationDuration::get(),
                0,
                TestSignature(1, vec![1, 2, 3, 4]),
                POOR_ACCOUNT
            ),
            redeem_code::Error::<Test>::InvalidSignature
        );

        assert_noop!(
            RedeemCode::name_redeem(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"cupnfishxxx".to_vec(),
                MinRegistrationDuration::get(),
                0,
                TestSignature(1, signature.clone()),
                POOR_ACCOUNT
            ),
            redeem_code::Error::<Test>::InvalidSignature
        );

        assert_noop!(
            RedeemCode::name_redeem(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"cupn---fish".to_vec(),
                MinRegistrationDuration::get(),
                0,
                TestSignature(OFFICIAL_ACCOUNT, signature.clone()),
                POOR_ACCOUNT
            ),
            redeem_code::Error::<Test>::InvalidSignature
        );

        assert_ok!(RedeemCode::name_redeem(
            RuntimeOrigin::signed(RICH_ACCOUNT),
            b"cupnfish".to_vec(),
            MinRegistrationDuration::get(),
            0,
            TestSignature(OFFICIAL_ACCOUNT, signature.clone()),
            POOR_ACCOUNT
        ));

        let test_node = label.encode_with_node(&DOT_BASENODE);

        assert!(Nft::is_owner(&POOR_ACCOUNT, (0, test_node)));

        assert_noop!(
            RedeemCode::name_redeem(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"cupnfish".to_vec(),
                MinRegistrationDuration::get(),
                0,
                TestSignature(OFFICIAL_ACCOUNT, signature),
                POOR_ACCOUNT
            ),
            redeem_code::Error::<Test>::RedeemsHasBeenUsed
        );

        let nouce = 1_u32;
        let duration = MinRegistrationDuration::get();

        let signature = (duration, nouce).encode();

        assert_noop!(
            RedeemCode::name_redeem_any(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"cupnfish".to_vec(),
                MinRegistrationDuration::get(),
                0,
                TestSignature(OFFICIAL_ACCOUNT, vec![1, 2, 3, 4]),
                POOR_ACCOUNT
            ),
            redeem_code::Error::<Test>::RedeemsHasBeenUsed
        );

        assert_noop!(
            RedeemCode::name_redeem_any(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"cup-nfi--sh".to_vec(),
                MinRegistrationDuration::get(),
                1,
                TestSignature(OFFICIAL_ACCOUNT, signature.clone()),
                POOR_ACCOUNT
            ),
            redeem_code::Error::<Test>::ParseLabelFailed
        );

        assert_noop!(
            RedeemCode::name_redeem_any(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"cupnfish".to_vec(),
                MinRegistrationDuration::get(),
                1,
                TestSignature(OFFICIAL_ACCOUNT, signature.clone()),
                POOR_ACCOUNT
            ),
            redeem_code::Error::<Test>::LabelLenInvalid
        );

        assert_ok!(Registrar::register(
            RuntimeOrigin::signed(RICH_ACCOUNT),
            b"cupnfishqqq".to_vec(),
            POOR_ACCOUNT,
        ));

        assert_noop!(
            RedeemCode::name_redeem_any(
                RuntimeOrigin::signed(RICH_ACCOUNT),
                b"cupnfishqqq".to_vec(),
                MinRegistrationDuration::get(),
                1,
                TestSignature(OFFICIAL_ACCOUNT, signature.clone()),
                POOR_ACCOUNT
            ),
            registrar::Error::<Test>::Occupied
        );

        assert_ok!(RedeemCode::name_redeem_any(
            RuntimeOrigin::signed(RICH_ACCOUNT),
            b"cupnfishxxx".to_vec(),
            MinRegistrationDuration::get(),
            1,
            TestSignature(OFFICIAL_ACCOUNT, signature),
            POOR_ACCOUNT
        ));

        let test_node = Label::new_with_len("cupnfishxxx".as_bytes())
            .unwrap()
            .0
            .encode_with_node(&DOT_BASENODE);

        assert!(Nft::is_owner(&POOR_ACCOUNT, (0, test_node)));
    })
}

#[test]
fn resolvers_test() {
    new_test_ext().execute_with(|| {
        assert_ok!(Registrar::register(
            RuntimeOrigin::signed(RICH_ACCOUNT),
            b"cupnfishxxx".to_vec(),
            MONEY_ACCOUNT,
        ));

        let node = Label::new_with_len("cupnfishxxx".as_bytes())
            .unwrap()
            .0
            .encode_with_node(&DOT_BASENODE);

        assert_ok!(Resolvers::set_text(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            node,
            TextKind::Email,
            b"cupnfish@qq.com".to_vec().into(),
        ));
        assert_ok!(Resolvers::set_text(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            node,
            TextKind::Url,
            b"www.baidu.com".to_vec().into(),
        ));
        assert_ok!(Resolvers::set_text(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            node,
            TextKind::Avatar,
            b"cupnfish".to_vec().into(),
        ));
        assert_ok!(Resolvers::set_text(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            node,
            TextKind::Description,
            b"A Rust programer.".to_vec().into(),
        ));
        assert_ok!(Resolvers::set_text(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            node,
            TextKind::Notice,
            b"test notice".to_vec().into(),
        ));
        assert_ok!(Resolvers::set_text(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            node,
            TextKind::Keywords,
            b"test,key,words,show".to_vec().into(),
        ));
        assert_ok!(Resolvers::set_text(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            node,
            TextKind::Twitter,
            b"twitter address".to_vec().into(),
        ));
        assert_ok!(Resolvers::set_text(
            RuntimeOrigin::signed(MONEY_ACCOUNT),
            node,
            TextKind::Github,
            b"github homepage".to_vec().into(),
        ));
    })
}

#[test]
fn label_test() {
    // 中文 test
    assert!(Label::new_with_len("中文域名暂不支持".as_bytes()).is_none());

    // white space test
    assert!(Label::new_with_len("hello world".as_bytes()).is_none());

    // dot test
    assert!(Label::new_with_len("hello.world".as_bytes()).is_none());

    // '-' test
    assert!(Label::new_with_len("-hello".as_bytes()).is_none());
    assert!(Label::new_with_len("he-llo".as_bytes()).is_none());
    assert!(Label::new_with_len("he--llo".as_bytes()).is_none());
    assert!(Label::new_with_len("hello-".as_bytes()).is_none());

    // normal label test
    assert!(Label::new_with_len("hello".as_bytes()).is_some());
    assert!(Label::new_with_len("111hello".as_bytes()).is_some());
    assert!(Label::new_with_len("123455".as_bytes()).is_some());
    assert!(Label::new_with_len("0x1241513".as_bytes()).is_some());

    // result test
    assert_eq!(
        Label::new_with_len("dot".as_bytes())
            .unwrap()
            .0
            .to_basenode(),
        DOT_BASENODE
    )
}
