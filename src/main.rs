use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use reqwest::blocking::{Client, multipart};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, Read, Write},
    path::PathBuf,
    time::Duration,
};

const SPEC: &str = include_str!("../spec/openapi.json");
#[derive(Clone)]
struct Operation {
    method: String,
    path: String,
    name: String,
    data: Value,
}
fn schema<'a>(s: &'a Value, v: &'a Value) -> &'a Value {
    v.get("$ref")
        .and_then(Value::as_str)
        .and_then(|r| s.pointer(r.trim_start_matches('#')))
        .unwrap_or(v)
}
fn load_spec() -> Value {
    let mut s: Value = serde_json::from_str(SPEC).expect("bundled schema");
    let extra: Value = serde_json::from_str(include_str!("../spec/ollama-request-schemas.json"))
        .expect("supplemental schemas");
    for (path, name) in [
        ("/api/chat", "OllamaChatRequest"),
        ("/api/generate", "OllamaGenerateRequest"),
    ] {
        s["paths"][path]["post"]["requestBody"] =
            json!({"required":true,"content":{"application/json":{"schema":extra[name].clone()}}});
    }
    s
}
fn operations(s: &Value) -> Vec<Operation> {
    let mut out = vec![];
    for (path, item) in s["paths"].as_object().unwrap() {
        for (method, data) in item.as_object().unwrap() {
            if !["get", "post", "put", "patch", "delete", "head", "options"]
                .contains(&method.as_str())
            {
                continue;
            }
            let name = format!(
                "{}-{}",
                method,
                path.trim_matches('/')
                    .replace(['/', '_'], "-")
                    .replace(['{', '}'], "")
                    .trim_matches('-')
            );
            out.push(Operation {
                method: method.to_uppercase(),
                path: path.clone(),
                name: if path == "/" { "get-root".into() } else { name },
                data: data.clone(),
            });
        }
    }
    out
}
fn body_schema<'a>(s: &'a Value, o: &'a Operation) -> (&'a str, &'a Value) {
    for t in [
        "application/json",
        "multipart/form-data",
        "application/x-www-form-urlencoded",
    ] {
        if let Some(v) = o.data.pointer(&format!(
            "/requestBody/content/{}/schema",
            t.replace('~', "~0").replace('/', "~1")
        )) {
            return (t, schema(s, v));
        }
    }
    ("application/json", &Value::Null)
}
fn sample(s: &Value, v: &Value, depth: usize) -> Value {
    let v = schema(s, v);
    if depth > 8 {
        return json!({});
    }
    if let Some(x) = v.get("default").filter(|x| !x.is_null()) {
        return x.clone();
    }
    if let Some(x) = v["enum"].as_array().and_then(|a| a.first()) {
        return x.clone();
    }
    if let Some(a) = v["anyOf"].as_array() {
        return sample(
            s,
            a.iter().find(|b| b["type"] != "null").unwrap_or(&a[0]),
            depth + 1,
        );
    }
    match v["type"].as_str().unwrap_or("object") {
        "string" => json!("example"),
        "integer" | "number" => v.get("minimum").cloned().unwrap_or(json!(1)),
        "boolean" => json!(true),
        "array" => json!([sample(s, &v["items"], depth + 1)]),
        _ => {
            let mut obj = serde_json::Map::new();
            for k in v["required"].as_array().into_iter().flatten() {
                if let Some(k) = k.as_str() {
                    obj.insert(k.into(), sample(s, &v["properties"][k], depth + 1));
                }
            }
            Value::Object(obj)
        }
    }
}
fn example_args(s: &Value, o: &Operation) -> Vec<String> {
    let mut a = vec![o.name.clone()];
    for p in o.data["parameters"].as_array().into_iter().flatten() {
        if p["required"] == true {
            a.push(format!(
                "--{}",
                p["name"].as_str().unwrap().replace('_', "-")
            ));
            let v = sample(s, &p["schema"], 0);
            a.push(
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string()),
            );
        }
    }
    let (ct, b) = body_schema(s, o);
    if ct == "multipart/form-data" {
        a.extend(["--file".into(), "./document.txt".into()]);
    } else if o.data.get("requestBody").is_some() {
        a.extend(["--body".into(), sample(s, b, 0).to_string()]);
    }
    a.extend(["--dry-run".into(), "--json".into()]);
    a
}
fn field_arg(name: &str, v: &Value, required: bool) -> Arg {
    let shape = if let Some(a) = v["anyOf"].as_array() {
        a.iter()
            .map(|x| x["type"].as_str().unwrap_or("object"))
            .collect::<Vec<_>>()
            .join(" | ")
    } else {
        v["type"].as_str().unwrap_or("object").into()
    };
    let mut help = format!(
        "{} [type: {}; {}]",
        v["description"]
            .as_str()
            .unwrap_or("API parameter; objects/arrays use JSON"),
        shape,
        if required { "required" } else { "optional" }
    );
    for key in [
        "enum",
        "default",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
    ] {
        if let Some(x) = v.get(key) {
            help.push_str(&format!(" [{key}: {x}]"));
        }
    }
    Arg::new(name.to_string())
        .long(if name == "dry_run" {
            "server-dry-run".to_string()
        } else {
            name.replace('_', "-")
        })
        .help(help)
        .num_args(1)
        .allow_hyphen_values(true)
}
fn cli(s: &Value, ops: &[Operation]) -> Command {
    let mut cmd = Command::new("lightrag-cli")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Complete, schema-driven LightRAG REST client")
        .subcommand_required(true)
        .arg(
            Arg::new("url")
                .long("url")
                .env("LIGHTRAG_URL")
                .default_value("http://localhost:9621")
                .global(true),
        )
        .arg(
            Arg::new("credentials-file")
                .long("credentials-file")
                .env("LIGHTRAG_CREDENTIALS_FILE")
                .global(true)
                .help("username:password file; defaults to ~/.config/lightrag"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .global(true),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(ArgAction::SetTrue)
                .global(true)
                .help("Build request without reading credentials or networking"),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .action(ArgAction::SetTrue)
                .global(true)
                .help("Explicitly permit mutating requests"),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .default_value("120")
                .value_parser(clap::value_parser!(u64))
                .global(true),
        )
        .subcommand(
            Command::new("operations")
                .about("List complete operation/parameter coverage")
                .after_help(
                    "Examples:\n  lightrag-cli operations\n  lightrag-cli operations --json",
                ),
        )
        .subcommand(
            Command::new("schema")
                .about("Print bundled OpenAPI schema (no credentials needed)")
                .after_help("Examples:\n  lightrag-cli schema\n  lightrag-cli schema --json"),
        );
    for o in ops {
        let mut c=Command::new(o.name.clone()).about(o.data["summary"].as_str().unwrap_or(&o.path).to_string()).long_about(format!("{} {}\n\n{}",o.method,o.path,o.data["description"].as_str().unwrap_or("")))
  .arg(Arg::new("body").long("body").help("Complete JSON body: literal, @file, or - for stdin"))
  .arg(Arg::new("output").long("output").help("Write exact response bytes to file (assets/streams)"))
  .after_help(format!("Examples:\n  lightrag-cli {} --help\n  lightrag-cli {}\n\nThe preview example supplies required fields. Replace sample values before real requests.\nUse --body @request.json for nested/complete JSON. Omitted fields preserve server defaults.\nMutations require --yes. Stream responses are NDJSON; --output preserves raw bytes.",o.name,example_args(s,o).iter().map(|a|if a.starts_with('{') || a.contains(' ') {format!("'{}'",a.replace('\'',"'\"'\"'"))}else{a.clone()}).collect::<Vec<_>>().join(" ")));
        if ["/documents/paginated", "/documents/source_conflicts"].contains(&o.path.as_str()) {
            c = c
                .arg(Arg::new("all").long("all").action(ArgAction::SetTrue).help(
                    "Fetch successive pages safely; explicit limits; partial results exit nonzero",
                ))
                .arg(
                    Arg::new("max-pages")
                        .long("max-pages")
                        .default_value("100")
                        .value_parser(clap::value_parser!(u64).range(1..=10000))
                        .help("Maximum pages with --all"),
                )
                .arg(
                    Arg::new("max-items")
                        .long("max-items")
                        .default_value("10000")
                        .value_parser(clap::value_parser!(u64).range(1..=1000000))
                        .help("Maximum retained items with --all"),
                );
        }
        for p in o.data["parameters"].as_array().into_iter().flatten() {
            c = c.arg(field_arg(
                p["name"].as_str().unwrap(),
                schema(s, &p["schema"]),
                p["required"] == true,
            ));
        }
        let (_, b) = body_schema(s, o);
        for (k, v) in b["properties"].as_object().into_iter().flatten() {
            if [
                "body",
                "output",
                "url",
                "json",
                "dry-run",
                "yes",
                "timeout",
                "credentials-file",
            ]
            .contains(&k.as_str())
            {
                continue;
            }
            c = c.arg(field_arg(
                k,
                schema(s, v),
                b["required"]
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(k))),
            ));
        }
        cmd = cmd.subcommand(c);
    }
    cmd
}
fn typed(s: &Value, v: &Value, raw: &str) -> Result<Value> {
    let v = schema(s, v);
    if let Some(a) = v.get("anyOf").and_then(Value::as_array) {
        if raw == "null" && a.iter().any(|v| v["type"] == "null") {
            return Ok(Value::Null);
        }
        for branch in a {
            if branch["type"] == "null" && raw == "null" {
                return Ok(Value::Null);
            }
            if let Ok(r) = typed(s, branch, raw) {
                return Ok(r);
            }
        }
        bail!("value does not match schema");
    }
    let r = match v["type"].as_str().unwrap_or("object") {
        "string" => Value::String(raw.into()),
        "integer" => json!(raw.parse::<i64>().context("expected integer")?),
        "number" => json!(raw.parse::<f64>().context("expected number")?),
        "boolean" => json!(raw.parse::<bool>().context("expected true or false")?),
        "null" => {
            if raw != "null" {
                bail!("expected null");
            }
            Value::Null
        }
        _ => serde_json::from_str(raw).context("expected JSON")?,
    };
    if let Some(en) = v["enum"].as_array()
        && !en.contains(&r)
    {
        bail!("value is not in allowed enum: {}", v["enum"]);
    }
    if let Some(n) = r.as_f64() {
        for (bound, lower, exclusive) in [
            ("minimum", true, false),
            ("maximum", false, false),
            ("exclusiveMinimum", true, true),
            ("exclusiveMaximum", false, true),
        ] {
            if let Some(b) = v[bound].as_f64()
                && ((lower && (n < b || (exclusive && n == b)))
                    || (!lower && (n > b || (exclusive && n == b))))
            {
                bail!("value violates {bound} {b}");
            }
        }
    }
    if let Some(text) = r.as_str() {
        let n = text.chars().count() as u64;
        if v["minLength"].as_u64().is_some_and(|b| n < b)
            || v["maxLength"].as_u64().is_some_and(|b| n > b)
        {
            bail!("string length violates schema bounds");
        }
    }
    if let Some(a) = r.as_array() {
        let n = a.len() as u64;
        if v["minItems"].as_u64().is_some_and(|b| n < b)
            || v["maxItems"].as_u64().is_some_and(|b| n > b)
        {
            bail!("array length violates schema bounds");
        }
    }
    Ok(r)
}
fn read_body(raw: &str) -> Result<Value> {
    let text = if raw == "-" {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        s
    } else if let Some(p) = raw.strip_prefix('@') {
        fs::read_to_string(p)?
    } else {
        raw.into()
    };
    serde_json::from_str(&text).context("invalid JSON body")
}
#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Value,
    content_type: String,
    files: BTreeMap<String, PathBuf>,
}
fn request(s: &Value, o: &Operation, m: &ArgMatches) -> Result<Request> {
    let mut r = Request {
        method: o.method.clone(),
        path: o.path.clone(),
        query: vec![],
        body: m
            .get_one::<String>("body")
            .map(|x| read_body(x))
            .transpose()?
            .unwrap_or(json!({})),
        content_type: body_schema(s, o).0.into(),
        files: BTreeMap::new(),
    };
    for p in o.data["parameters"].as_array().into_iter().flatten() {
        let name = p["name"].as_str().unwrap();
        if let Some(raw) = m.get_one::<String>(name) {
            typed(s, &p["schema"], raw)?;
            match p["in"].as_str() {
                Some("path") => {
                    let enc: String =
                        url::form_urlencoded::byte_serialize(raw.as_bytes()).collect();
                    r.path = r
                        .path
                        .replace(&format!("{{{name}}}"), &enc.replace('+', "%20"));
                }
                Some("query") => r.query.push((name.into(), raw.clone())),
                _ => bail!("unsupported parameter location"),
            }
        } else if p["required"] == true {
            bail!("missing required --{}", name.replace('_', "-"));
        }
    }
    let (_, b) = body_schema(s, o);
    if b["type"] == "object" && !r.body.is_object() {
        bail!("JSON body must be an object");
    }
    for (k, v) in b["properties"].as_object().into_iter().flatten() {
        if let Ok(Some(raw)) = m.try_get_one::<String>(k) {
            if schema(s, v)["format"] == "binary"
                || schema(s, v)["contentMediaType"] == "application/octet-stream"
            {
                r.files.insert(k.clone(), raw.into());
            } else {
                r.body[k] = typed(s, v, raw)
                    .with_context(|| format!("invalid --{}", k.replace('_', "-")))?;
            }
        }
    }
    for k in b["required"].as_array().into_iter().flatten() {
        let k = k.as_str().unwrap();
        if r.body.get(k).is_none() && !r.files.contains_key(k) {
            bail!(
                "missing required --{} or body field {k}",
                k.replace('_', "-")
            );
        }
    }
    Ok(r)
}
fn is_read(o: &Operation) -> bool {
    o.method == "GET"
        || [
            "/query",
            "/query/data",
            "/query/stream",
            "/documents/paginated",
            "/api/generate",
            "/api/chat",
            "/login",
        ]
        .contains(&o.path.as_str())
}
fn client(timeout: u64) -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(timeout))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
fn auth(c: &Client, base: &str, m: &ArgMatches) -> Result<Option<(String, String)>> {
    if let Ok(key) = std::env::var("LIGHTRAG_API_KEY") {
        return Ok(Some(("X-API-Key".into(), key)));
    }
    if let Ok(token) = std::env::var("LIGHTRAG_TOKEN") {
        return Ok(Some(("Authorization".into(), format!("Bearer {token}"))));
    }
    let path = m
        .get_one::<String>("credentials-file")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config/lightrag")
        });
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).context("cannot read credential file")?;
    let (user, password) = text
        .trim()
        .split_once(':')
        .context("credential file must contain username:password")?;
    let res = c
        .post(format!("{base}/login"))
        .form(&[("username", user), ("password", password)])
        .send()
        .context("login transport failed")?;
    if !res.status().is_success() {
        bail!(
            "login failed (HTTP {}); verify credentials",
            res.status().as_u16()
        );
    }
    let data: Value = res.json().context("login response is not JSON")?;
    let token = data["access_token"]
        .as_str()
        .context("login response missing access_token")?;
    Ok(Some(("Authorization".into(), format!("Bearer {token}"))))
}
fn paginate(
    c: &Client,
    base: &str,
    authentication: Option<(String, String)>,
    r: &Request,
    m: &ArgMatches,
) -> Result<()> {
    let max_pages = *m.get_one::<u64>("max-pages").unwrap();
    let max_items = *m.get_one::<u64>("max-items").unwrap() as usize;
    let documents = r.path == "/documents/paginated";
    let mut page = r.body["page"].as_u64().unwrap_or(1);
    let mut cursor = r
        .query
        .iter()
        .find(|(k, _)| k == "cursor")
        .map(|(_, v)| v.clone());
    let mut seen_cursors = std::collections::HashSet::new();
    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_pages = std::collections::HashSet::new();
    let mut items = Vec::new();
    let mut pages = 0;
    let mut duplicates = 0;
    let mut complete = false;
    let mut failure = None;
    let mut offset = 0;
    while pages < max_pages && items.len() < max_items {
        if let Some(cur) = &cursor
            && !seen_cursors.insert(cur.clone())
        {
            failure = Some("repeated pagination cursor".to_string());
            break;
        }
        let result = (|| -> Result<Value> {
            let mut body = r.body.clone();
            let mut query = r.query.clone();
            if documents {
                body["page"] = json!(page);
                if body.get("page_size").is_none() {
                    body["page_size"] = json!(200);
                }
            } else {
                query.retain(|(k, _)| k != "cursor");
                if let Some(cur) = &cursor {
                    query.push(("cursor".into(), cur.clone()));
                }
            }
            let mut b = c
                .request(r.method.parse()?, format!("{base}{}", r.path))
                .query(&query);
            if let Some((k, v)) = &authentication {
                b = b.header(k, v);
            }
            if documents {
                b = b.json(&body);
            }
            let response = b.send().context("pagination transport failure")?;
            if !response.status().is_success() {
                bail!("pagination HTTP {}", response.status().as_u16());
            }
            response.json().context("invalid pagination JSON")
        })();
        let v = match result {
            Ok(v) => v,
            Err(e) => {
                failure = Some(e.to_string());
                break;
            }
        };
        pages += 1;
        let key = if documents { "documents" } else { "conflicts" };
        let Some(batch) = v[key].as_array() else {
            failure = Some("pagination response missing item array".into());
            break;
        };
        if !batch.is_empty() && !seen_pages.insert(serde_json::to_string(batch)?) {
            failure = Some("repeated pagination page".into());
            break;
        }
        let mut truncated = false;
        for (i, item) in batch.iter().enumerate() {
            let id = item[if documents {
                "id"
            } else {
                "canonical_source_key"
            }]
            .as_str();
            if id.is_some_and(|id| !seen_ids.insert(id.to_string())) {
                duplicates += 1;
                continue;
            }
            if items.len() == max_items {
                offset = i;
                truncated = true;
                break;
            }
            items.push(item.clone());
        }
        if truncated {
            failure = Some("item limit reached".into());
            break;
        }
        if documents {
            let Some(has_next) = v["pagination"]["has_next"].as_bool() else {
                failure = Some("pagination response missing has_next".into());
                break;
            };
            if v["pagination"]["page"].as_u64() != Some(page) {
                failure = Some("server returned unexpected page number".into());
                break;
            }
            if has_next && batch.is_empty() {
                failure = Some("empty document page claims continuation".into());
                break;
            }
            if !has_next {
                complete = true;
                break;
            }
            page += 1;
        } else {
            match v.get("next_cursor") {
                Some(Value::Null) => {
                    complete = true;
                    break;
                }
                Some(Value::String(next)) if !next.is_empty() => cursor = Some(next.clone()),
                _ => {
                    failure = Some("pagination response missing valid next_cursor".into());
                    break;
                }
            }
        }
    }
    if !complete && failure.is_none() {
        failure = Some("pagination safety limit reached".into());
    }
    let result = json!({"items":items,"pages_fetched":pages,"complete":complete,"duplicates_skipped":duplicates,"error":failure,"resume":if complete{Value::Null}else if documents{json!({"page":page,"item_offset":offset})}else{json!({"cursor":cursor,"item_offset":offset})},"snapshot_consistent":false});
    let bytes = if m.get_flag("json") {
        serde_json::to_vec(&result)?
    } else {
        serde_json::to_vec_pretty(&result)?
    };
    if let Some(path) = m.get_one::<String>("output") {
        fs::write(path, bytes)?;
    } else {
        io::stdout().write_all(&bytes)?;
        println!();
    }
    if !complete {
        bail!("pagination incomplete; partial results and continuation preserved");
    }
    Ok(())
}
fn run() -> Result<()> {
    let s = load_spec();
    let ops = operations(&s);
    let matches = cli(&s, &ops).get_matches();
    let (name, m) = matches.subcommand().unwrap();
    if name == "schema" {
        println!("{SPEC}");
        return Ok(());
    }
    if name == "operations" {
        let rows:Vec<_>=ops.iter().map(|o|json!({"command":o.name,"method":o.method,"path":o.path,"operation_id":o.data["operationId"],"parameters":o.data["parameters"],"body_schema":body_schema(&s,o).1,"requires_yes":!is_read(o)})).collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    let o = ops.iter().find(|o| o.name == name).unwrap();
    let r = request(&s, o, m)?;
    if matches.get_flag("dry-run") {
        println!(
            "{}",
            json!({"dry_run":true,"method":r.method,"path":r.path,"query":r.query,"body":if o.path=="/login"{json!("[redacted]")}else{r.body.clone()},"files":r.files,"content_type":r.content_type,"requires_yes":!is_read(o),"automatic_pagination":m.try_get_one::<bool>("all").ok().flatten().copied().unwrap_or(false),"pagination_limits":{"max_pages":m.try_get_one::<u64>("max-pages").ok().flatten(),"max_items":m.try_get_one::<u64>("max-items").ok().flatten()}})
        );
        return Ok(());
    }
    if !is_read(o) && !matches.get_flag("yes") {
        bail!("mutation requires --yes; preview first with --dry-run");
    }
    let base = matches
        .get_one::<String>("url")
        .unwrap()
        .trim_end_matches('/');
    let parsed = url::Url::parse(base).context("invalid base URL")?;
    if !["http", "https"].contains(&parsed.scheme())
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        bail!("base URL must be http(s), without credentials, query or fragment");
    }
    let c = client(*matches.get_one::<u64>("timeout").unwrap())?;
    if m.try_get_one::<bool>("all")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false)
    {
        return paginate(&c, base, auth(&c, base, &matches)?, &r, m);
    }
    let mut b = c
        .request(r.method.parse()?, format!("{base}{}", r.path))
        .query(&r.query);
    if o.path != "/login"
        && let Some((key, value)) = auth(&c, base, &matches)?
    {
        b = b.header(key, value);
    }
    if r.content_type == "multipart/form-data" {
        let mut form = multipart::Form::new();
        for (k, v) in r.body.as_object().context("body must be object")? {
            if r.files.contains_key(k) {
                continue;
            }
            form = form.text(
                k.clone(),
                v.as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string()),
            );
        }
        for (k, p) in r.files {
            form = form.file(k, p)?;
        }
        b = b.multipart(form);
    } else if r.content_type == "application/x-www-form-urlencoded" {
        let fields: BTreeMap<_, _> = r
            .body
            .as_object()
            .context("body must be object")?
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| v.to_string()),
                )
            })
            .collect();
        b = b.form(&fields);
    } else if r.body != json!({}) || o.data.get("requestBody").is_some() {
        b = b.json(&r.body);
    }
    let mut res = b.send().context("request transport failed")?;
    let status = res.status();
    if status.is_redirection() && o.path == "/" {
        let location = res
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        println!(
            "{}",
            json!({"status":status.as_u16(),"location":location,"followed":false})
        );
        return Ok(());
    }
    if !status.is_success() {
        bail!(
            "HTTP {} for {} {} (response omitted to avoid leaking data); no automatic retry",
            status.as_u16(),
            r.method,
            r.path
        );
    }
    let ct = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|x| x.to_str().ok())
        .unwrap_or("")
        .to_string();
    if o.path == "/login" {
        let v: Value = res.json()?;
        let safe = json!({"authenticated":v.get("access_token").is_some(),"token_type":v["token_type"],"note":"Token omitted; use credential file for automatic login"});
        if let Some(p) = m.get_one::<String>("output") {
            fs::write(p, serde_json::to_vec(&safe)?)?;
        } else {
            println!("{safe}");
        }
    } else if ct.contains("ndjson") || ct.contains("event-stream") || o.path == "/query/stream" {
        let mut out: Box<dyn Write> = if let Some(p) = m.get_one::<String>("output") {
            Box::new(fs::File::create(p)?)
        } else {
            Box::new(io::stdout())
        };
        let mut reader = io::BufReader::new(res);
        let mut line = Vec::new();
        let mut failed = false;
        while reader.read_until(b'\n', &mut line)? != 0 {
            if !line.iter().all(u8::is_ascii_whitespace) {
                match serde_json::from_slice::<Value>(&line) {
                    Ok(v) if v.is_object() => {
                        if v.get("error").is_some_and(|e| !e.is_null()) {
                            failed = true;
                        }
                    }
                    _ => failed = true,
                }
            }
            out.write_all(&line)?;
            out.flush()?;
            line.clear();
        }
        if failed {
            bail!("invalid NDJSON or server error within stream; partial output preserved");
        }
    } else if let Some(p) = m.get_one::<String>("output") {
        let mut file = fs::File::create(p)?;
        io::copy(&mut res, &mut file)?;
    } else if ct.contains("json") {
        let v: Value = res.json()?;
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string(&v)?);
        } else {
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
    } else {
        io::copy(&mut res, &mut io::stdout().lock())?;
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_preview_example_is_valid() {
        let s = load_spec();
        let ops = operations(&s);
        for o in &ops {
            let mut args = vec!["lightrag-cli".to_string()];
            args.extend(example_args(&s, o));
            let m = cli(&s, &ops).try_get_matches_from(args).unwrap();
            request(&s, o, m.subcommand().unwrap().1)
                .unwrap_or_else(|e| panic!("{}: {}", o.name, e));
        }
    }
    #[test]
    fn all_commands_build() {
        let s = load_spec();
        let ops = operations(&s);
        assert_eq!(ops.len(), 45);
        cli(&s, &ops).debug_assert();
    }
    #[test]
    fn enum_validation() {
        assert!(typed(&json!({}), &json!({"type":"string","enum":["a"]}), "b").is_err());
    }
    #[test]
    fn required_path() {
        let s = load_spec();
        let ops = operations(&s);
        let m = cli(&s, &ops)
            .try_get_matches_from(["lightrag-cli", "get-documents-track-status-track-id"])
            .unwrap();
        let o = ops
            .iter()
            .find(|o| o.path == "/documents/track_status/{track_id}")
            .unwrap();
        assert!(request(&s, o, m.subcommand().unwrap().1).is_err());
    }
}
