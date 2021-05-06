use bikecleats_web_test_utils::Credentials;
use insta::{assert_json_snapshot, assert_snapshot};

#[test]
fn atcoder() -> anyhow::Result<()> {
    let (messages, outcome) =
        bikecleats_web_test_utils::run::<Credentials, _, _>(|sess, credentials| {
            sess.atcoder_login(credentials.atcoder())
        })?;
    assert_snapshot!("atcoder_messages", messages);
    assert_json_snapshot!("atcoder_outcome", outcome);
    Ok(())
}
