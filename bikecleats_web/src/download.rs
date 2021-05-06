use anyhow::Context as _;
use futures_util::StreamExt as _;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::convert::TryInto as _;
use tokio::runtime::Runtime;
use unicode_width::UnicodeWidthStr as _;

// https://github.com/rust-lang/rust-clippy/issues/5991
#[allow(clippy::needless_collect)]
pub(super) fn download_with_progress(
    draw_target: ProgressDrawTarget,
    dl_targets: Vec<(String, reqwest::RequestBuilder)>,
) -> anyhow::Result<Vec<String>> {
    let rt = Runtime::new()?;
    let mp = MultiProgress::with_draw_target(draw_target);
    let name_width = dl_targets.iter().map(|(s, _)| s.width()).max().unwrap_or(0);

    let handles = dl_targets
        .into_iter()
        .map(|(name, req)| {
            let pb = mp.add(ProgressBar::new(0));
            pb.set_style(progress_style("{prefix:.bold} Waiting..."));
            pb.set_prefix(align_left(&name, name_width));

            rt.spawn(async move {
                let res = req.send().await?;

                tokio::task::block_in_place(|| {
                    if let Some(content_len) = res.content_length() {
                        pb.set_length(content_len);
                    }

                    pb.set_style(progress_style(
                        "{prefix:.bold} {bytes:9} {bytes_per_sec:11} {elapsed_precise} {bar} \
                         {percent}%",
                    ));
                });

                let mut content = vec![];
                let mut stream = res.bytes_stream();

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;

                    content.extend_from_slice(chunk.as_ref());

                    tokio::task::block_in_place(|| {
                        pb.inc(chunk.len().try_into().unwrap_or(u64::MAX));
                    });
                }

                tokio::task::block_in_place(|| pb.finish_at_current_pos());

                reqwest::Result::Ok(content)
            })
        })
        .collect::<Vec<_>>();

    mp.join()?;

    return handles
        .into_iter()
        .map(|handle| {
            String::from_utf8(rt.block_on(handle)??).with_context(|| "invalid UTF-8 content")
        })
        .collect();

    fn progress_style(template: &str) -> ProgressStyle {
        ProgressStyle::default_bar().template(template)
    }

    fn align_left(s: &str, n: usize) -> String {
        let spaces = n.saturating_sub(s.width());
        s.chars().chain(itertools::repeat_n(' ', spaces)).collect()
    }
}
