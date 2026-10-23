use crate::domain::{Post, UserId};
use crate::ports::{PostStore, UserStore};

use super::Network;

impl<U: UserStore, P: PostStore> Network<U, P> {
    pub fn feed(&self, viewer: UserId) -> impl Iterator<Item = &Post> + '_ {
        self.posts
            .all()
            .iter()
            .rev()
            .filter(move |post| self.is_in_feed(viewer, post))
    }

    pub fn search_tag(&self, viewer: UserId, tag: &str) -> impl Iterator<Item = &Post> + '_ {
        self.tags
            .posts(tag)
            .rev()
            .filter_map(|id| self.posts.get(id))
            .filter(move |post| self.is_shown(viewer, post))
    }

    pub fn post_count_of(&self, author: UserId) -> usize {
        self.posts_of(author).count()
    }

    pub fn likes_received(&self, author: UserId) -> usize {
        self.posts_of(author).map(Post::likes).sum()
    }

    fn posts_of(&self, author: UserId) -> impl Iterator<Item = &Post> + '_ {
        self.posts
            .all()
            .iter()
            .filter(move |post| post.author() == author && !post.is_deleted())
    }

    fn is_shown(&self, viewer: UserId, post: &Post) -> bool {
        let author = post.author();
        !post.is_deleted()
            && !self.mutes.contains(viewer, author)
            && post.can_view(viewer, self.follows.contains(viewer, author))
    }

    fn is_in_feed(&self, viewer: UserId, post: &Post) -> bool {
        let author = post.author();
        let followed = author == viewer || self.follows.contains(viewer, author);
        followed && self.is_shown(viewer, post)
    }
}

#[cfg(test)]
mod tests {
    use crate::{MemNetwork, PostId, UserId, Visibility};

    fn network() -> (MemNetwork, UserId, UserId) {
        let mut net = MemNetwork::new();
        let alice = net.register("alice").expect("alice регистрируется");
        let bob = net.register("bob").expect("bob регистрируется");
        (net, alice, bob)
    }

    fn publish(net: &mut MemNetwork, author: UserId, text: &str, visibility: Visibility) -> PostId {
        net.publish(author, text, visibility)
            .expect("пост публикуется")
    }

    #[test]
    fn feed_without_follows_has_only_own_posts() {
        let (mut net, alice, bob) = network();
        let own = publish(&mut net, alice, "моё", Visibility::Private);
        publish(&mut net, bob, "чужое", Visibility::Public);
        let ids: Vec<PostId> = net.feed(alice).map(|post| post.id()).collect();
        assert_eq!(ids, vec![own]);
    }

    #[test]
    fn unknown_viewer_gets_empty_feed() {
        let (mut net, alice, _) = network();
        publish(&mut net, alice, "всем", Visibility::Public);
        assert!(net.feed(UserId(99)).next().is_none());
    }

    #[test]
    fn muted_and_deleted_posts_leave_the_feed() {
        let (mut net, alice, bob) = network();
        net.follow(alice, bob).expect("подписка проходит");
        let gone = publish(&mut net, alice, "удалю", Visibility::Public);
        publish(&mut net, bob, "скрою", Visibility::Public);
        net.delete_post(alice, gone)
            .expect("автор удаляет свой пост");
        net.mute(alice, bob).expect("alice скрывает bob");
        assert!(net.feed(alice).next().is_none());
        assert_eq!(net.post_count_of(alice), 0);
    }

    #[test]
    fn likes_received_sums_only_author_posts() {
        let (mut net, alice, bob) = network();
        let first = publish(&mut net, alice, "раз", Visibility::Public);
        let second = publish(&mut net, alice, "два", Visibility::Public);
        let foreign = publish(&mut net, bob, "чужой", Visibility::Public);
        for post in [first, second, foreign] {
            net.like(bob, post).expect("лайк проходит");
        }
        net.like(alice, second).expect("лайк проходит");
        assert_eq!(net.likes_received(alice), 3);
        assert_eq!(net.post_count_of(alice), 2);
    }
}
