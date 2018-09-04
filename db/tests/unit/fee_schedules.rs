use bigneon_db::models::FeeSchedule;
use support::project::TestProject;

#[test]
fn fee_schedule_create() {
    let db = TestProject::new();
    let fee_schedule = FeeSchedule::create("default".to_string(), vec![(0, 200), (10_000, 100)])
        .commit(&db)
        .unwrap();

    let ranges = fee_schedule.ranges(&db).unwrap();
    assert_eq!(
        vec![ranges[0].min_price, ranges[1].min_price],
        vec![0, 10_000]
    );
    assert_eq!(vec![ranges[0].fee, ranges[1].fee], vec![200, 100]);
}