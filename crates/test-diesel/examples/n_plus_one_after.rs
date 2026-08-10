//! Fix for the N+1 query shown in the `n_plus_one_before` example: posts and
//! their comment counts are fetched in a single JOIN + GROUP BY query. The SQL
//! report shows one call attributed to `list_comments`
//! instead of one per post.
//!
//! Run with:
//!   cargo run -p test-diesel --example n_plus_one_after --features hotpath

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

diesel::joinable!(comments -> posts (post_id));
diesel::allow_tables_to_appear_in_same_query!(posts, comments);

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

#[hotpath::measure]
fn list_comments(
    conn: &mut SqliteConnection,
) -> Result<Vec<(String, i64)>, Box<dyn std::error::Error>> {
    let rows: Vec<(String, i64)> = posts::table
        .left_join(comments::table)
        .group_by((posts::id, posts::title))
        .select((posts::title, diesel::dsl::count(comments::id.nullable())))
        .order(posts::id.asc())
        .load(conn)?;
    Ok(rows)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    hotpath::instrument_diesel_sql();

    let _guard = HotpathGuardBuilder::new("main")
        .sections(vec![Section::FunctionsTiming, Section::Sql])
        .build();

    let mut conn = SqliteConnection::establish(":memory:")?;
    seed(&mut conn)?;

    for (title, count) in list_comments(&mut conn)? {
        println!("{title}: {count} comments");
    }
    Ok(())
}
