use anyhow::Context;
use config::{any_err, get_or_create_sub_module, serialize_options};
use dns_resolver::{
    get_resolver, resolve_a_or_aaaa, set_mx_concurrency_limit, set_mx_negative_cache_ttl,
    set_mx_timeout, HickoryResolver, MailExchanger, TestResolver, UnboundResolver,
};
use hickory_resolver::config::{NameServerConfig, ResolveHosts, ResolverConfig, ResolverOpts};
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::proto::xfer::Protocol;
use hickory_resolver::{Name, TokioResolver};
use mlua::{Lua, LuaSerdeExt, Value};
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

pub fn register(lua: &Lua) -> anyhow::Result<()> {
    let dns_mod = get_or_create_sub_module(lua, "dns")?;

    dns_mod.set(
        "lookup_mx",
        lua.create_async_function(|lua, domain: String| async move {
            let mx = MailExchanger::resolve(&domain).await.map_err(any_err)?;
            Ok(lua.to_value_with(&*mx, serialize_options()))
        })?,
    )?;

    dns_mod.set(
        "set_mx_concurrency_limit",
        lua.create_function(move |_lua, limit: usize| {
            set_mx_concurrency_limit(limit);
            Ok(())
        })?,
    )?;

    dns_mod.set(
        "set_mx_timeout",
        lua.create_function(move |lua, duration: Value| {
            let duration: duration_serde::Wrap<Duration> = lua.from_value(duration)?;
            set_mx_timeout(duration.into_inner()).map_err(any_err)
        })?,
    )?;

    dns_mod.set(
        "set_mx_negative_cache_ttl",
        lua.create_function(move |lua, duration: Value| {
            let duration: duration_serde::Wrap<Duration> = lua.from_value(duration)?;
            set_mx_negative_cache_ttl(duration.into_inner()).map_err(any_err)
        })?,
    )?;

    dns_mod.set(
        "lookup_ptr",
        lua.create_async_function(|lua, ip_str: String| async move {
            let resolver = get_resolver();
            let addr = std::net::IpAddr::from_str(&ip_str).map_err(any_err)?;
            let answer = resolver.resolve_ptr(addr).await.map_err(any_err)?;
            Ok(lua.to_value_with(&*answer, serialize_options()))
        })?,
    )?;

    dns_mod.set(
        "lookup_txt",
        lua.create_async_function(|_lua, domain: String| async move {
            let resolver = get_resolver();
            let answer = resolver.resolve_txt(&domain).await.map_err(any_err)?;
            Ok(answer.as_txt())
        })?,
    )?;

    dns_mod.set(
        "lookup_addr",
        lua.create_async_function(|_lua, domain: String| async move {
            let result = resolve_a_or_aaaa(&domain).await.map_err(any_err)?;
            let result: Vec<String> = result
                .into_iter()
                .map(|item| item.addr.to_string())
                .collect();
            Ok(result)
        })?,
    )?;

    #[derive(serde::Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct DnsConfig {
        #[serde(default)]
        domain: Option<String>,
        #[serde(default)]
        search: Vec<String>,
        #[serde(default)]
        name_servers: Vec<NameServer>,
        #[serde(default)]
        options: ResolverOpts,
    }

    #[derive(serde::Deserialize, Debug)]
    #[serde(untagged)]
    #[serde(deny_unknown_fields)]
    enum NameServer {
        Ip(String),
        Detailed {
            socket_addr: String,
            #[serde(default)]
            protocol: Protocol,
            #[serde(default)]
            trust_negative_responses: bool,
            #[serde(default)]
            bind_addr: Option<String>,
        },
    }

    dns_mod.set(
        "configure_resolver",
        lua.create_function(move |lua, config: mlua::Value| {
            let config: DnsConfig = lua.from_value(config)?;

            let mut r_config = ResolverConfig::new();
            if let Some(dom) = config.domain {
                r_config.set_domain(
                    Name::from_str_relaxed(&dom)
                        .with_context(|| format!("domain: '{dom}'"))
                        .map_err(any_err)?,
                );
            }
            for s in config.search {
                let name = Name::from_str_relaxed(&s)
                    .with_context(|| format!("search: '{s}'"))
                    .map_err(any_err)?;
                r_config.add_search(name);
            }

            for ns in config.name_servers {
                r_config.add_name_server(match ns {
                    NameServer::Ip(ip) => {
                        let ip: SocketAddr = ip
                            .parse()
                            .with_context(|| format!("name server: '{ip}'"))
                            .map_err(any_err)?;
                        NameServerConfig::new(ip, Protocol::Udp)
                    }
                    NameServer::Detailed {
                        socket_addr,
                        protocol,
                        trust_negative_responses,
                        bind_addr,
                    } => {
                        let ip: SocketAddr = socket_addr
                            .parse()
                            .with_context(|| format!("name server: '{socket_addr}'"))
                            .map_err(any_err)?;
                        let mut c = NameServerConfig::new(ip, protocol);

                        c.trust_negative_responses = trust_negative_responses;

                        if let Some(bind) = bind_addr {
                            let addr: SocketAddr = bind
                                .parse()
                                .with_context(|| {
                                    format!("name server: '{socket_addr}' bind_addr: '{bind}'")
                                })
                                .map_err(any_err)?;
                            c.bind_addr.replace(addr);
                        }

                        c
                    }
                });
            }

            let mut builder =
                TokioResolver::builder_with_config(r_config, TokioConnectionProvider::default());
            *builder.options_mut() = config.options;
            dns_resolver::reconfigure_resolver(HickoryResolver::from(builder.build()));

            Ok(())
        })?,
    )?;

    dns_mod.set(
        "configure_unbound_resolver",
        lua.create_function(move |lua, config: mlua::Value| {
            let config: DnsConfig = lua.from_value(config)?;

            let context = libunbound::Context::new().map_err(any_err)?;

            for ns in config.name_servers {
                let addr = match ns {
                    NameServer::Ip(ip) => ip
                        .parse()
                        .with_context(|| format!("name server: '{ip}'"))
                        .map_err(any_err)?,
                    NameServer::Detailed { socket_addr, .. } => socket_addr
                        .parse()
                        .with_context(|| format!("name server: '{socket_addr}'"))
                        .map_err(any_err)?,
                };
                context
                    .set_forward(Some(addr))
                    .context("set_forward")
                    .map_err(any_err)?;
            }

            // TODO: expose a way to provide unbound configuration
            // options to this code

            if config.options.validate {
                context
                    .add_builtin_trust_anchors()
                    .context("add_builtin_trust_anchors")
                    .map_err(any_err)?;
            }
            if matches!(
                config.options.use_hosts_file,
                ResolveHosts::Always | ResolveHosts::Auto
            ) {
                context
                    .load_hosts(None)
                    .context("load_hosts")
                    .map_err(any_err)?;
            }

            let context = context
                .into_async()
                .context("make async resolver context")
                .map_err(any_err)?;

            dns_resolver::reconfigure_resolver(UnboundResolver::from(context));

            Ok(())
        })?,
    )?;

    dns_mod.set(
        "configure_test_resolver",
        lua.create_function(move |_lua, zones: Vec<String>| {
            let mut resolver = TestResolver::default();
            for zone in &zones {
                resolver = resolver.with_zone(zone);
            }

            dns_resolver::reconfigure_resolver(resolver);
            Ok(())
        })?,
    )?;

    Ok(())
}
