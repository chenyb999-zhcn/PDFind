// 知识库: SQLite 存储 (bundled, 含 FTS5 trigram 中文分词)
use rusqlite::{params, Connection};
use std::path::PathBuf;
use tauri::Manager;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS documents(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,            -- video | pdf | text
  title TEXT NOT NULL,
  source_path TEXT NOT NULL,
  pdf_path TEXT,                 -- video: 生成的 PDF 路径
  lang TEXT NOT NULL DEFAULT 'zh',
  duration_s REAL NOT NULL DEFAULT 0,
  meta TEXT NOT NULL DEFAULT '{}',
  created_at INTEGER NOT NULL,
  n_chunks INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS chunks(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  doc_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  chapter TEXT NOT NULL DEFAULT '',
  text TEXT NOT NULL,
  start_s REAL NOT NULL DEFAULT 0,
  end_s REAL NOT NULL DEFAULT 0,
  page INTEGER NOT NULL DEFAULT 0,
  embedding BLOB
);
CREATE INDEX IF NOT EXISTS idx_chunks_doc ON chunks(doc_id);
-- trigram tokenizer: 支持中文子串匹配 (查询需 >=3 字符)
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
  text, content='chunks', content_rowid='id', tokenize='trigram'
);
CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
  INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
  INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE OF text ON chunks BEGIN
  INSERT INTO chunks_fts(chunks_fts, rowid, text) VALUES ('delete', old.id, old.text);
  INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
"#;

// 每次命令调用开一个连接 (WAL 模式下并发读安全, 避免跨线程共享)
pub struct Db {
    pub conn: Connection,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocInfo {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub source_path: String,
    pub pdf_path: Option<String>,
    pub lang: String,
    pub duration_s: f64,
    pub n_chunks: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct ChunkRow {
    pub id: i64,
    pub doc_id: i64,
    pub seq: i64,
    pub chapter: String,
    pub text: String,
    pub start_s: f64,
    pub end_s: f64,
    pub page: i64,
    pub embedding: Option<Vec<u8>>,
}

pub fn db_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("kb.sqlite")
}

impl Db {
    pub fn open(app: &tauri::AppHandle) -> Result<Self, String> {
        let p = db_path(app);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(&p).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| e.to_string())?;
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(Self { conn })
    }

    // ============ documents ============
    pub fn insert_doc(
        &self,
        kind: &str,
        title: &str,
        source_path: &str,
        pdf_path: Option<&str>,
        lang: &str,
        duration_s: f64,
        meta: &str,
    ) -> Result<i64, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn
            .execute(
                "INSERT INTO documents(kind,title,source_path,pdf_path,lang,duration_s,meta,created_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![kind, title, source_path, pdf_path, lang, duration_s, meta, now],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_docs(&self) -> Result<Vec<DocInfo>, String> {
        let mut st = self
            .conn
            .prepare("SELECT id,kind,title,source_path,pdf_path,lang,duration_s,n_chunks,created_at
                      FROM documents ORDER BY created_at DESC")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| {
                Ok(DocInfo {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    title: r.get(2)?,
                    source_path: r.get(3)?,
                    pdf_path: r.get(4)?,
                    lang: r.get(5)?,
                    duration_s: r.get(6)?,
                    n_chunks: r.get(7)?,
                    created_at: r.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn remove_doc(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM documents WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // 同 kind + 来源路径的文档 (重加入库时覆盖)
    pub fn find_doc_by_source(&self, kind: &str, source_path: &str) -> Result<Option<i64>, String> {
        match self.conn.query_row(
            "SELECT id FROM documents WHERE kind=?1 AND source_path=?2 LIMIT 1",
            params![kind, source_path],
            |r| r.get(0),
        ) {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn set_doc_chunks(&self, id: i64, n: i64) -> Result<(), String> {
        self.conn
            .execute("UPDATE documents SET n_chunks=?2 WHERE id=?1", params![id, n])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // ============ chunks ============
    pub fn insert_chunk(
        &self,
        doc_id: i64,
        seq: i64,
        chapter: &str,
        text: &str,
        start_s: f64,
        end_s: f64,
        page: i64,
        embedding: Option<&[u8]>,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO chunks(doc_id,seq,chapter,text,start_s,end_s,page,embedding)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![doc_id, seq, chapter, text, start_s, end_s, page, embedding],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    // 带向量的全部块 (检索用; 库规模千级, 全量加载足够)
    pub fn chunks_with_embedding(&self) -> Result<Vec<ChunkRow>, String> {
        let mut st = self
            .conn
            .prepare("SELECT id,doc_id,seq,chapter,text,start_s,end_s,page,embedding
                      FROM chunks WHERE embedding IS NOT NULL")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], |r| {
                Ok(ChunkRow {
                    id: r.get(0)?,
                    doc_id: r.get(1)?,
                    seq: r.get(2)?,
                    chapter: r.get(3)?,
                    text: r.get(4)?,
                    start_s: r.get(5)?,
                    end_s: r.get(6)?,
                    page: r.get(7)?,
                    embedding: r.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    // 按 id 取块 (含无向量的)
    pub fn chunk_by_id(&self, id: i64) -> Option<ChunkRow> {
        self.conn
            .query_row(
                "SELECT id,doc_id,seq,chapter,text,start_s,end_s,page,embedding
                 FROM chunks WHERE id=?1",
                params![id],
                |r| {
                    Ok(ChunkRow {
                        id: r.get(0)?,
                        doc_id: r.get(1)?,
                        seq: r.get(2)?,
                        chapter: r.get(3)?,
                        text: r.get(4)?,
                        start_s: r.get(5)?,
                        end_s: r.get(6)?,
                        page: r.get(7)?,
                        embedding: r.get(8)?,
                    })
                },
            )
            .ok()
    }

    pub fn doc_title(&self, doc_id: i64) -> Option<String> {
        self.conn
            .query_row(
                "SELECT title FROM documents WHERE id=?1",
                params![doc_id],
                |r| r.get(0),
            )
            .ok()
    }

    // FTS trigram 检索: 返回 (rowid, bm25 分数), 查询串需 >=3 字符
    pub fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f64)>, String> {
        let q = query.replace('"', "\"\"");
        let q = format!("\"{q}\"");
        let mut st = self
            .conn
            .prepare("SELECT rowid, bm25(chunks_fts) FROM chunks_fts WHERE chunks_fts MATCH ?1
                      ORDER BY bm25(chunks_fts) LIMIT ?2")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![q, limit as i64], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn count_chunks(&self) -> Result<i64, String> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        Ok(n)
    }
}
