# zproxy

zproxy es un proxy local escrito en Rust que escucha en localhost y reenvía tráfico HTTP y HTTPS usando una cadena de resolución configurable:

1. upstream principal opcional
2. fallback opcional
3. salida directa cuando la configuración lo permite

La implementación actual ya soporta:

- Proxy HTTP local con soporte para peticiones HTTP normales.
- Soporte CONNECT para túneles HTTPS sin MITM.
- Upstream estático por HTTP o SOCKS5.
- Upstream dinámico basado en el gateway por defecto del sistema.
- Fallback estático o directo.
- Daemon con pidfile, lockfile y socket Unix de control.
- Wizard interactivo para generar la configuración TOML.

La implementación actual no incluye todavía:

- Registro automático como proxy del sistema.
- Plantillas de LaunchAgent o systemd.
- Lanzador para .zshrc o .bashrc.
- Reglas de bypass por dominio o IP.
- Encadenamiento multinivel de fallbacks.
- Autenticación contra upstream proxies.

## Estructura

- [docs/quickstart.md](docs/quickstart.md): instalación mínima y primer arranque.
- [docs/configuration.md](docs/configuration.md): formato de configuración y ejemplos.
- [docs/operations.md](docs/operations.md): daemon, socket de control, logs y operación diaria.
- [macos-proxy-rust-design.md](macos-proxy-rust-design.md): diseño original y backlog de evolución.

## Comandos disponibles

El binario expone estos subcomandos:

- `zproxy daemon`: arranca el daemon y el listener del proxy local.
- `zproxy config`: abre el wizard interactivo, guarda la config y pide recarga al daemon si está corriendo.
- `zproxy status`: consulta el estado actual por el socket Unix.
- `zproxy reload`: recarga la configuración desde disco.
- `zproxy stop`: detiene el daemon.
- `zproxy service install`: instala el daemon como servicio de usuario (LaunchAgent en macOS, systemd --user en Linux).
- `zproxy service start`: inicia el servicio instalado.
- `zproxy service restart`: reinicia el servicio instalado.
- `zproxy service status`: consulta estado del gestor de servicios (instalado/activo).
- `zproxy service stop`: detiene el servicio instalado.
- `zproxy service logs [--lines N] [--follow]`: muestra logs del servicio (tail/journalctl según plataforma).
- `zproxy service uninstall`: desinstala el servicio de usuario.
- `zproxy start`: si hay servicio instalado, lo inicia; si no, pregunta si quieres arrancar en modo detached.
- `zproxy start --detached`: arranca `zproxy daemon` en background sin instalar servicio.
- `zproxy logs [--lines N] [--follow] [--detached]`: muestra logs del servicio si está instalado; si no, hace tail de `zproxy.log` (modo detached).
- `zproxy paths`: imprime rutas de config, estado, socket y pidfile.

## Resumen operativo

El daemon usa estas rutas por defecto:

- Configuración: `~/.config/zproxy/config.toml`
- Estado: `~/.local/state/zproxy`
- Socket de control: `~/.local/state/zproxy/zproxy.sock`
- PID: `~/.local/state/zproxy/zproxy.pid`
- Lock: `~/.local/state/zproxy/zproxy.lock`

Cuando el upstream se configura como `gateway`, zproxy detecta periódicamente la IP del gateway por defecto y reconstruye el upstream efectivo como `gateway_ip:port`. En macOS usa `route -n get default`; en Linux intenta primero `/proc/net/route` y, si no basta, `ip route show default`.

## Desarrollo

Comandos útiles:

```bash
cargo fmt --check
cargo check
cargo run -- paths
cargo run -- config
cargo run -- daemon
```

Si el cache global de Cargo queda bloqueado por otro proceso del sistema, se puede aislar la comprobación con:

```bash
CARGO_HOME=$PWD/.cargo-home cargo check
```

## Estado del proyecto

Esta base ya sirve como prototipo funcional para probar routing local, CONNECT y resolución de upstream/fallback. La parte más incompleta hoy está en la integración con el sistema operativo y el packaging de servicio.