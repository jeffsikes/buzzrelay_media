use std::sync::Arc;
use tokio_postgres::{Client, Error, NoTls, Statement};

use crate::actor;

const CREATE_SCHEMA_COMMANDS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS follows (id TEXT, inbox TEXT, actor TEXT, UNIQUE (inbox, actor))",
    "CREATE INDEX ON follows (actor) INCLUDE (inbox)",
];

#[derive(Clone)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

struct DatabaseInner {
    client: Client,
    add_follow: Statement,
    del_follow: Statement,
    get_following_inboxes: Statement,
}

impl Database {
    pub async fn connect(conn_str: &str) -> Self {
        let (client, connection) = tokio_postgres::connect(conn_str, NoTls)
            .await
            .unwrap();

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!("postgresql: {}", e);
            }
        });

        for command in CREATE_SCHEMA_COMMANDS {
            client.execute(*command, &[])
                .await
                .unwrap();
        }
        let add_follow = client.prepare("INSERT INTO follows (id, inbox, actor) VALUES ($1, $2, $3)")
            .await
            .unwrap();
        let del_follow = client.prepare("DELETE FROM follows WHERE id=$1 AND actor=$2")
            .await
            .unwrap();
        let get_following_inboxes = client.prepare("SELECT DISTINCT inbox FROM follows WHERE actor=$1")
            .await
            .unwrap();

        Database {
            inner: Arc::new(DatabaseInner {
                client,
                add_follow,
                del_follow,
                get_following_inboxes,
            }),
        }
    }

    pub async fn add_follow(&self, id: &str, inbox: &str, actor: &str) -> Result<(), Error> {
        self.inner.client.execute(&self.inner.add_follow, &[&id, &inbox, &actor])
            .await?;
        Ok(())
    }

    pub async fn del_follow(&self, id: &str, actor: &str) -> Result<(), Error> {
        self.inner.client.execute(&self.inner.del_follow, &[&id, &actor])
            .await?;
        Ok(())
    }

    pub async fn get_following_inboxes(&self, actor: &str) -> Result<impl Iterator<Item = String>, Error> {
        let rows = self.inner.client.query(&self.inner.get_following_inboxes, &[&actor])
            .await?;
        Ok(rows.into_iter()
           .map(|row| row.get(0))
        )
    }
}