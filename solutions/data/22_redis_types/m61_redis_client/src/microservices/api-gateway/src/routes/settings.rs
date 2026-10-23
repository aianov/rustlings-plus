use contracts::{ErrorBody, FoundView, HitView, PeopleView, SettingsView, TagView, TagsView};
use proto::social::v1::{FindRequest, OwnProfileRequest, PeopleQuery, Settings, TopRequest};

use super::Ctx;
use crate::common::Principal;
use crate::dto::{QueryAsk, TopAsk, post_view};

const USERS: &str = "сервис профилей";

const POSTS: &str = "сервис постов";

fn view(settings: Settings) -> SettingsView {
    SettingsView {
        theme: settings.theme,
        language: settings.language,
        private: settings.private,
        digest: settings.digest,
    }
}

pub(super) async fn find(ctx: Ctx, data: QueryAsk) -> Result<FoundView, ErrorBody> {
    let ask = FindRequest {
        query: data.query,
        limit: data.limit,
    };
    let reply = ctx.posts().find_posts(ask).await;
    let reply = reply.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(FoundView {
        hits: reply
            .hits
            .into_iter()
            .filter_map(|hit| {
                Some(HitView {
                    post: post_view(hit.post?),
                    rank: hit.rank,
                    headline: hit.headline,
                })
            })
            .collect(),
    })
}

pub(super) async fn people(ctx: Ctx, data: QueryAsk) -> Result<PeopleView, ErrorBody> {
    let ask = PeopleQuery {
        query: data.query,
        limit: data.limit,
    };
    let found = ctx.users().search_people(ask).await;
    super::social::people(&ctx, 0, found)
}

pub(super) async fn read(
    ctx: Ctx,
    _: Principal,
    _: SettingsView,
) -> Result<SettingsView, ErrorBody> {
    let reply = ctx.users().get_settings(OwnProfileRequest {}).await;
    Ok(view(reply.map_err(ctx.failed(USERS))?.into_inner()))
}

pub(super) async fn write(
    ctx: Ctx,
    _: Principal,
    data: SettingsView,
) -> Result<SettingsView, ErrorBody> {
    let patch = Settings {
        theme: data.theme,
        language: data.language,
        private: data.private,
        digest: data.digest,
    };
    let reply = ctx.users().update_settings(patch).await;
    Ok(view(reply.map_err(ctx.failed(USERS))?.into_inner()))
}

pub(super) async fn tags(ctx: Ctx, data: TopAsk) -> Result<TagsView, ErrorBody> {
    let reply = ctx.posts().top_tags(TopRequest { limit: data.limit }).await;
    let reply = reply.map_err(ctx.failed(POSTS))?.into_inner();
    Ok(TagsView {
        tags: reply
            .tags
            .into_iter()
            .map(|tag| TagView {
                tag: tag.tag,
                count: tag.count,
            })
            .collect(),
    })
}

pub(super) async fn guests(
    ctx: Ctx,
    _: Principal,
    _: SettingsView,
) -> Result<PeopleView, ErrorBody> {
    let found = ctx.users().guests(OwnProfileRequest {}).await;
    super::social::people(&ctx, 0, found)
}
