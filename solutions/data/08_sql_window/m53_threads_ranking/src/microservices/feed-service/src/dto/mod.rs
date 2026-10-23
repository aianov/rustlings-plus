use proto::social::v1::{FeedRequest, Link as LinkReply, Post as PostReply};
use validation::{Invalid, Range, Validator};

use crate::common::FeedItem;

pub const MAX_PAGE_SIZE: usize = 100;

pub fn page(request: &FeedRequest) -> Result<(usize, usize), Invalid> {
    let mut check = Validator::new();
    let page = usize::try_from(request.page).unwrap_or(usize::MAX);
    let size = usize::try_from(request.size).unwrap_or(usize::MAX);
    let page_fits = check.check(
        "page",
        &page,
        &Range {
            min: 1,
            max: usize::MAX,
        },
    );
    let size_fits = check.check(
        "size",
        &size,
        &Range {
            min: 1,
            max: MAX_PAGE_SIZE,
        },
    );
    check.finish_with((page_fits && size_fits).then_some((page, size)))
}

pub fn post(item: &FeedItem) -> PostReply {
    PostReply {
        liked: false,
        latest: Vec::new(),
        id: item.post.0,
        author_id: item.author.0,
        author_name: item.author_name.clone(),
        text: item.text.clone(),
        likes: item.likes as u64,
        tags: item.tags.clone(),
        links: item
            .links
            .iter()
            .map(|link| LinkReply {
                url: link.url.clone(),
                title: link.title.clone().unwrap_or_default(),
            })
            .collect(),
    }
}
