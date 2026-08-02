//! Demonstrates spotting an N+1 query with SQL tracing: listing posts with
//! their comment counts issues one query for the posts plus one query per
//! post. In the SQL report the per-post query shows up as a single bucket
//! with `Calls` equal to the number of posts, attributed to
//! `list_posts_with_comment_counts`. Compare with the `n_plus_one_after`
//! example.
//!
//! Run with:
//!   cargo run -p test-diesel --example n_plus_one_before --features hotpath

use diesel::prelude::*;
use hotpath::{HotpathGuardBuilder, Section};

diesel::table! {
    posts (id) {
        id -> Integer,
        title -> Text,
    }
}

diesel::table! {
    comments (id) {
        id -> Integer,
        post_id -> Integer,
        body -> Text,
    }
}

#[derive(Queryable, Insertable)]
#[diesel(table_name = posts)]
struct Post {
    id: i32,
    title: String,
}

#[derive(Queryable, Insertable)]
#[diesel(table_name = comments)]
struct Comment {
    id: i32,
    post_id: i32,
    body: String,
}

fn seed(conn: &mut SqliteConnection) -> Result<(), Box<dyn std::error::Error>> {
    diesel::sql_query("CREATE TABLE posts (id INTEGER PRIMARY KEY, title TEXT NOT NULL)")
        .execute(conn)?;
    diesel::sql_query(
        "CREATE TABLE comments (id INTEGER PRIMARY KEY, post_id INTEGER NOT NULL, body TEXT NOT NULL)",
    )
    .execute(conn)?;

    for post_id in 1..=20 {
        diesel::insert_into(posts::table)
            .values(Post {
                id: post_id,
                title: format!("Post {post_id}"),
            })
            .execute(conn)?;
        for i in 1..=5 {
            diesel::insert_into(comments::table)
                .values(Comment {
                    id: (post_id - 1) * 5 + i,
                    post_id,
                    body: format!("Comment {i} on post {post_id}"),
                })
                .execute(conn)?;
        }
    }
    Ok(())
}

// N+1: one query for the posts, then one more per post to load its comments.
#[hotpath::measure]
fn list_posts_with_comment_counts(
    conn: &mut SqliteConnection,
) -> Result<Vec<(String, usize)>, Box<dyn std::error::Error>> {
    let all_posts: Vec<Post> = posts::table.load(conn)?;

    let mut result = Vec::with_capacity(all_posts.len());
    for post in all_posts {
        let post_comments: Vec<Comment> = comments::table
            .filter(comments::post_id.eq(post.id))
            .load(conn)?;
        result.push((post.title, post_comments.len()));
    }
    Ok(result)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::instrument_diesel_sql();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::FunctionsTiming, Section::Sql])
        .build();

    let mut conn = SqliteConnection::establish(":memory:")?;
    seed(&mut conn)?;

    for (title, count) in list_posts_with_comment_counts(&mut conn)? {
        println!("{title}: {count} comments");
    }
    Ok(())
}
