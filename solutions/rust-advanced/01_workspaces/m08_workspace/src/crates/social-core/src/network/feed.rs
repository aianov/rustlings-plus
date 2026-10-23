use crate::domain::{Post, UserId};
use crate::ports::{PostStore, UserStore};

use super::Network;

impl<U: UserStore, P: PostStore> Network<U, P> {
    pub fn feed(&self, viewer: UserId, limit: usize) -> Vec<&Post> {
        self.posts
            .all()
            .iter()
            .rev()
            .filter(|post| self.is_in_feed(viewer, post))
            .take(limit)
            .collect()
    }

    pub fn search_tag(&self, viewer: UserId, tag: &str) -> Vec<&Post> {
        self.tags
            .posts(tag)
            .iter()
            .rev()
            .filter_map(|id| self.posts.get(*id))
            .filter(|post| self.can_see(viewer, post))
            .collect()
    }

    pub fn post_count_of(&self, author: UserId) -> usize {
        self.posts
            .all()
            .iter()
            .filter(|post| post.author() == author)
            .count()
    }

    pub fn likes_received(&self, author: UserId) -> usize {
        self.posts
            .all()
            .iter()
            .filter(|post| post.author() == author)
            .map(Post::likes)
            .sum()
    }

    fn can_see(&self, viewer: UserId, post: &Post) -> bool {
        post.can_view(viewer, self.follows.contains(viewer, post.author()))
    }

    fn is_in_feed(&self, viewer: UserId, post: &Post) -> bool {
        let author = post.author();
        let followed = author == viewer || self.follows.contains(viewer, author);
        followed && self.can_see(viewer, post)
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
        let ids: Vec<PostId> = net.feed(alice, 10).iter().map(|post| post.id()).collect();
        assert_eq!(ids, vec![own]);
    }

    #[test]
    fn unknown_viewer_gets_empty_feed() {
        let (mut net, alice, _) = network();
        publish(&mut net, alice, "всем", Visibility::Public);
        assert!(net.feed(UserId(99), 10).is_empty());
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
