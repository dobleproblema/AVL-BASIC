# Rust 1.95.0 frente a 1.98.1 — 25–26 de septiembre de 2026

**Actualización del 26 de septiembre:** el usuario instaló las nuevas herramientas
y se completó la [comparación de MSVC](MSVC-2026-09-26.md). El bloqueo de instalación
descrito a continuación corresponde a esta primera fase, ya superada.

## Resultado

Actualizar Rust no recupera por sí solo el rendimiento de Windows. Con la misma
fuente y la separación de funciones introducida en el estudio anterior, Rust
1.98.1 tarda aproximadamente un 3,5 % más en Pi y un 1,4 % más en Jelly con ventana.
En WSL/Linux, el cambio de compilador reduce el tiempo de Pi un 2,16 % y el de
Jelly sin ventana un 4,57 %.

Una variante adicional de Windows con Rust 1.98.1 y sin los dos atributos
`link_section` mejora respecto a conservarlos, pero frente a la configuración
actual deja un resultado mixto: reducción geométrica del tiempo del 0,67 % en
ocho cargas, con regresiones en Pi, raytracer y matrices. No se considera una
recuperación general ni una garantía de estabilidad frente a futuras ediciones.

En esta primera fase, la comparación con MSVC actualizado no pudo realizarse.
La revisión automática rechazó iniciar el instalador con elevación normal de Windows
(`Start-Process -Verb RunAs`, respuesta «blocked by policy»). No se inició UAC ni
se instaló MSVC. Se dejó preparado el instalador oficial firmado y un procedimiento
manual; no se intentó eludir el rechazo. La instalación posterior del usuario
permitió completar la comparación documentada en el informe del 26 de septiembre.

No se han sustituido los ejecutables de desarrollo, cambiado el compilador por
defecto, retirado los atributos de las fuentes activas, ni publicado una versión.

## Variables y procedencia

- Fuente: AVL-BASIC 1.6.6, HEAD `b9b64e3`, con los dos atributos de sección del
  [estudio de colocación](CODE-PLACEMENT-2026-09-25.md).
- Rust/Cargo 1.95.0 y LLVM 22.1.2 frente a Rust/Cargo 1.98.1 y LLVM 22.1.8.
- Ambas herramientas Rust quedan instaladas en paralelo en Windows y WSL.
  El canal `stable` predeterminado sigue siendo 1.95.0. La instalación de Windows
  actualizó automáticamente rustup de 1.29.0 a 1.29.1; esto no cambia los binarios
  usados en las mediciones. WSL conserva rustup 1.29.0.
- MSVC fijado: toolset 14.35.32215, `link.exe` 14.35.32217.1, de Community 17.5.5.
- Windows SDK fijado: 10.0.22000.0. Mismas rutas de bibliotecas e includes, y
  selección explícita de `link.exe`, `cl.exe` y `lib.exe` por ruta absoluta.
- Mismos 33 hashes de fuentes, `Cargo.lock` y manifiesto entre compiladores;
  mismos cuatro outputs generados por el build. `cargo build --release --locked`
  en directorios separados. Perfil sin cambios: nivel 3, ThinLTO, una unidad de
  generación, `panic=abort`, `strip=symbols`.
- El control Windows 1.95 recompilado conserva byte por byte todas las secciones
  ejecutables del binario de desarrollo previamente entregado.
- En Linux se recompilaron ambas versiones desde la misma fuente; se verificó
  también la misma versión de `cc` y los mismos hashes de entradas.

Las descargas oficiales Linux sufrían cortes. Se recuperaron por rangos HTTPS,
verificando los SHA-256 del manifiesto oficial; rustup instaló los archivos desde
un espejo local temporal mediante su mecanismo normal de comprobación. No se
desactivó TLS ni la comprobación de integridad. El registro está en
`target/toolchain-comparison-20260925/wsl-rust1981-dist/installation.md`.

## Método

Se reutiliza `representative.py` y su configuración fija. Pi calcula 10.000
dígitos; Jelly dibuja 10.000 puntos durante 320 frames sin ventana o 1.600 con
ventana. Se alterna AB/BA con un calentamiento y 4 o 6 repeticiones medidas por
ejecutable; las cargas rotan de orden. Windows usa el procesador lógico 2 mediante
`compare-lp2.py`; Linux usa `taskset -c 2`, un procesador virtual cuya numeración
no debe equipararse a la afinidad física de Windows.

La métrica primaria es la mediana del tiempo externo del proceso. Los FPS de
ventana son `1600 / mediana(body_s)`: se comprobó que todos esos tiempos internos
son positivos e inferiores al tiempo externo. No se mezclan las dos métricas.
Cada ejecución verifica la salida normalizada y el PNG final, no todos los frames.

No hubo builds, tests ni otros benchmarks durante las mediciones. Las descargas
de Linux continuaron durante las primeras tandas Windows y parte de la ablación.
Por ese motivo se repitieron Pi/Jelly y la ventana después de finalizar todas las
descargas e instalaciones; el resultado conservó la dirección y magnitud.
Las tandas combinadas y Linux también se ejecutaron después de terminar la instalación.

## Efecto aislado de cambiar Rust en Windows

Misma fuente, con las dos secciones. Suite inicial: 6 repeticiones por ejecutable.
En las tablas, un incremento de tiempo es una regresión.

| Programa | Rust 1.95, s | Rust 1.98.1, s | Cambio de tiempo |
|---|---:|---:|---:|
| Raytracer | 2,276644 | 2,311157 | +1,52 % |
| Pi Machin | 1,755817 | 1,815652 | +3,41 % |
| N-Queens | 1,384191 | 1,352149 | −2,31 % |
| Jelly sin ventana | 0,991154 | 1,003531 | +1,25 % |
| Mandelbrot | 0,647449 | 0,681736 | +5,30 % |
| Cadenas | 1,378722 | 1,372460 | −0,45 % |
| Pi moderno | 2,081755 | 2,100416 | +0,90 % |
| Matrices | 0,996619 | 1,015155 | +1,86 % |

La razón geométrica indica un incremento del tiempo del 1,41 %. Las diferencias
pequeñas, especialmente texto y matrices, requieren cautela por su dispersión.
Jelly con ventana: 5,106455 → 5,170692 s; 315,08 → 311,33 FPS.

Repetición posterior sin descargas, 4 repeticiones por ejecutable:

| Caso | Rust 1.95 | Rust 1.98.1 | Cambio de tiempo |
|---|---:|---:|---:|
| Pi, tiempo externo | 1,762092 s | 1,824503 s | +3,54 % |
| Jelly sin ventana | 1,014459 s | 1,024484 s | +0,99 % |
| Jelly con ventana | 5,062120 s | 5,131287 s | +1,37 % |

Los FPS de esta segunda tanda son 317,90 → 313,35. La variación de los valores
absolutos entre tandas es otra razón para usar comparaciones alternadas dentro
de cada tanda, en vez de comparar ejecuciones aisladas separadas en el tiempo.

## Interacción con la separación de funciones

Se preparó una copia aislada quitando únicamente los dos atributos `link_section`.
El archivo de Git usa LF y el checkout usa CRLF; se comprobaron las fuentes
normalizando solo ese detalle. Un control adicional, compilado desde esa misma
copia con los atributos restaurados, produjo secciones ejecutables idénticas a
las del build original de Rust 1.98.1. Se descarta así un efecto de la ruta de la
copia sobre el código medido. Los outputs generados también son iguales.

Dentro de Rust 1.98.1, quitar los atributos reduce el tiempo de Pi de 1,805149 a
1,762852 s (2,34 %) y Jelly de 1,001077 a 0,975147 s (2,59 %), en 6 repeticiones.

Comparación de la configuración actual (Rust 1.95 con secciones) frente a la
candidata (Rust 1.98.1 sin secciones), 4 repeticiones:

| Programa | Actual, s | Candidata, s | Cambio de tiempo |
|---|---:|---:|---:|
| Raytracer | 2,260115 | 2,279620 | +0,86 % |
| Pi Machin | 1,746525 | 1,766735 | +1,16 % |
| N-Queens | 1,381640 | 1,368534 | −0,95 % |
| Jelly sin ventana | 0,984772 | 0,972134 | −1,28 % |
| Mandelbrot | 0,648356 | 0,617223 | −4,80 % |
| Cadenas | 1,356372 | 1,356087 | −0,02 % |
| Pi moderno | 2,079852 | 2,050656 | −1,40 % |
| Matrices | 0,982130 | 0,993753 | +1,18 % |

Reducción geométrica del tiempo: 0,67 %. Con ventana: 5,055028 → 5,030717 s
(0,48 % menos tiempo externo), 318,71 → 319,72 FPS. Esta comparación cambia
compilador y atributos: **no representa el efecto aislado de actualizar Rust**.
No demuestra que eliminar las secciones estabilice las futuras compilaciones.

## Linux y corrección

WSL Ubuntu 24.04, misma fuente y 6 repeticiones:

| Programa | Rust 1.95, s | Rust 1.98.1, s | Cambio de tiempo |
|---|---:|---:|---:|
| Pi Machin | 2,259949 | 2,211088 | −2,16 % |
| Jelly sin ventana | 1,031395 | 0,984271 | −4,57 % |

Los atributos estudiados están restringidos a Windows x64 MSVC, por lo que no
intervienen en esta comparación Linux.

- Windows Rust 1.98.1 con secciones: 765 tests pasan, 7 ignorados, cero fallos.
- Windows Rust 1.98.1 sin secciones: 765 pasan, 7 ignorados, cero fallos.
- WSL Rust 1.98.1: 775 pasan, 1 ignorado, cero fallos.
- Suites ejecutadas con `cargo test --locked --all-targets -- --test-threads=1`.
  Las pruebas opcionales que requieren `AVL_BASIC_PY_REPO` no se habilitaron.
- Los oráculos independientes de `7^10000` y Unicode coinciden en las 8
  ejecuciones de la candidata Windows y su referencia. Son casos ya conocidos,
  no una reserva nueva. No se optimizó ni midió el intérprete Python.

## Qué sabemos sobre el código emitido

En Windows, con ambas secciones, Rust 1.98.1 reduce el ejecutable en 48.128 bytes.
El bucle pasa de 20.035 a 19.615 bytes y de 3.753 a 3.702 instrucciones; el
evaluador, de 6.155 a 5.932 bytes y de 1.317 a 1.271 instrucciones. Se mantiene la
alineación de página, aunque cambia el orden de las secciones. Ambos binarios
pasan las comprobaciones de permisos RX y registros `.pdata`/unwind.

Menor tamaño y menos instrucciones estáticas no implican menor tiempo de ejecución.
Estos resultados no identifican un mecanismo concreto de caché o predicción ni
demuestran un límite de tamaño del compilador. Tampoco separan las contribuciones
de rustc, LLVM y la biblioteca estándar dentro de la actualización de Rust.

## Evidencias y estado al terminar la primera fase

[JSON de resultados y procedencia](toolchain-results-2026-09-25.json) conserva
medianas, dispersión, ejecuciones sin el texto voluminoso, hashes, versiones,
fuentes y auditorías. Los datos crudos, binarios, PDB y logs están en
`target/toolchain-comparison-20260925/`:

- `rust195-old.exe`: control recompilado.
- `rust198-old.exe`: cambio aislado de Rust, conserva secciones.
- `rust198-no-sections.exe`: candidata experimental sin secciones.
- `linux195` y `linux198`: controles de WSL.
- `build_windows.py`, `build_linux.py`, `summarize.py`: comandos y manifiestos
  reproducibles de las pruebas locales.
- `msvc-install/PLAN.md`: procedimiento preparado para instalar Build Tools 2022,
  manteniendo Community antiguo como control y el SDK 10.0.22000.0.

Al terminar esta primera fase faltaban Rust 1.95 y 1.98.1 con MSVC nuevo. Esa
comparación se completó después de la instalación, repitiendo también las
referencias: [informe de MSVC](MSVC-2026-09-26.md). La
[decisión final](BUILD-CHOICE-2026-09-26.md) registra la selección posterior de
compiladores y ejecutables de trabajo.
