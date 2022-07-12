use reg_index;
use serde_json;

#[test]
fn test_features2() {
    let input = include_str!("input_features2");

    for pkg_json in input.lines() {
        let pkg: reg_index::IndexPackage = serde_json::from_str(pkg_json).unwrap();
        assert!(pkg.features2.is_some());
        assert_eq!(pkg.v, Some(2));

        assert_eq!(pkg_json, serde_json::to_string(&pkg).unwrap());
    }
}
