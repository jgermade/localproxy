# Configuration

zproxy guarda la configuración en formato TOML en `~/.config/zproxy/config.toml`.

## Esquema general

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "none"

[fallback]
type = "direct"
```

## Sección listen

Parámetros soportados:

- `host`: IP de escucha local.
- `port`: puerto de escucha local.

Ejemplo:

```toml
[listen]
host = "127.0.0.1"
port = 8888
```

## Sección upstream

Tipos soportados:

- `none`
- `gateway`
- `static`

### Upstream none

No usa upstream principal.

```toml
[upstream]
type = "none"
```

### Upstream gateway

Construye el upstream como `gateway_actual:port` y vuelve a detectar el gateway periódicamente.

Campos soportados:

- `type = "gateway"`
- `protocol = "http" | "socks5"`
- `port`
- `poll_interval_secs`
- `connect_timeout_ms`

Ejemplo:

```toml
[upstream]
type = "gateway"
protocol = "http"
port = 8080
poll_interval_secs = 5
connect_timeout_ms = 3000
```

### Upstream static

Campos soportados:

- `type = "static"`
- `protocol = "http" | "socks5"`
- `host`
- `port`
- `connect_timeout_ms`

Ejemplo:

```toml
[upstream]
type = "static"
protocol = "socks5"
host = "127.0.0.1"
port = 1080
connect_timeout_ms = 3000
```

## Sección fallback

Tipos soportados:

- `none`
- `direct`
- `static`

### Fallback none

Si el upstream falla y el fallback es `none`, la conexión termina con error.

```toml
[fallback]
type = "none"
```

### Fallback direct

Si el upstream falla, zproxy intenta conectar directamente al destino.

```toml
[fallback]
type = "direct"
```

### Fallback static

Si el upstream falla, zproxy intenta un segundo proxy estático.

Campos soportados:

- `type = "static"`
- `protocol = "http" | "socks5"`
- `host`
- `port`
- `connect_timeout_ms`

Ejemplo:

```toml
[fallback]
type = "static"
protocol = "http"
host = "10.0.0.20"
port = 8080
connect_timeout_ms = 3000
```

## Orden de resolución real

La lógica efectiva del binario actual es esta:

1. Si hay upstream resoluble, lo intenta primero.
2. Si hay fallback estático, lo intenta después.
3. Si el fallback es `direct` o no hay upstream configurado, intenta salida directa.
4. Si nada funciona, devuelve `502 Bad Gateway` al cliente.

## Ejemplos completos

### Proxy local con salida directa

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "none"

[fallback]
type = "direct"
```

### Proxy local con upstream gateway y fallback directo

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "gateway"
protocol = "http"
port = 8080
poll_interval_secs = 5
connect_timeout_ms = 3000

[fallback]
type = "direct"
```

### Proxy local con upstream SOCKS5 estático y fallback HTTP estático

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "static"
protocol = "socks5"
host = "127.0.0.1"
port = 1080
connect_timeout_ms = 3000

[fallback]
type = "static"
protocol = "http"
host = "10.10.10.10"
port = 8080
connect_timeout_ms = 3000
```

## Consideraciones

- El wizard actual siempre propone `connect_timeout_ms = 3000`; si necesitas otro valor, edita el TOML manualmente.
- No hay validaciones avanzadas de reachability ni de autenticación del upstream.
- No existen reglas de bypass por dominio, CIDR o sufijo.