# Verificación de la publicación 1.6.7 — 26 de septiembre de 2026

La versión definitiva conserva el comportamiento medido de la configuración
elegida, con pequeñas diferencias entre programas. En Windows, la media
geométrica de tiempos aumenta un 0,42 %; Pi aumenta un 1,20 % y Jelly con ventana
reduce su tiempo externo un 0,51 %. En WSL, Pi y Jelly sin ventana cambian menos
del 0,2 %. No se reproduce una caída comparable a la regresión grande anterior.
No se atribuyen automáticamente estas pequeñas diferencias a ruido ni se afirma
que todas las compilaciones futuras vayan a mantener el mismo rendimiento.

**La pérdida histórica respecto a 1.5.96 continúa incompletamente explicada.**
Esta publicación incorpora la recuperación parcial estudiada para Windows;
no se presenta como recuperación total de aquella referencia.

## Qué se compara

La referencia de esta tanda es la **1.6.6 local ya corregida**, con las herramientas
seleccionadas, y el candidato es el ejecutable final **1.6.7**. No es una nueva
comparación directa con el publicado 1.6.6 ni con 1.5.96.

- Fuente de publicación: `8c0cba2`, versión 1.6.7. Los únicos cambios del intérprete
  desde el publicado 1.6.6 son los dos atributos Windows de sección, descritos en
  el [estudio del código](CODE-PLACEMENT-2026-09-25.md).
- Windows: Rust/Cargo 1.95.0, MSVC 14.44.35229.0, SDK 10.0.22000.0.
- WSL Ubuntu 24.04: Rust/Cargo 1.98.1 y las herramientas nativas registradas en el
  manifiesto. Mismo perfil release que antes, sin nuevas opciones de optimización.
- Un calentamiento y cuatro repeticiones medidas por ejecutable y carga;
  alternancia AB/BA, cargas y tamaños fijos. Las mediciones empiezan después de
  finalizar compilaciones, suites y paridad, y se realizan sucesivamente.
- Afinidad Windows al procesador lógico 2 y WSL a la CPU virtual 2. No se asume
  que ambos identifiquen el mismo núcleo físico. Los porcentajes comparan
  ejecutables dentro de cada sistema, no el coste de Windows frente a WSL.
- 110 ejecuciones incluyendo calentamientos: 80 de la batería Windows,
  10 de Jelly con ventana Windows y 20 de Pi/Jelly sin ventana WSL.

Se verifican la salida normalizada y el PNG final de cada comparación; no todos
los fotogramas intermedios. La métrica principal es el tiempo externo monotónico
del proceso. Los FPS se obtienen del tiempo del cuerpo de Jelly.

## Windows

Medianas en segundos; un cambio positivo indica mayor tiempo.

| Programa | 1.6.6 local elegida | 1.6.7 final | Cambio |
| --- | ---: | ---: | ---: |
| Raytracer | 2,257887 | 2,288228 | +1,34 % |
| Pi, 10.000 dígitos | 1,756135 | 1,777148 | +1,20 % |
| N-Queens | 1,376486 | 1,382226 | +0,42 % |
| Jelly sin ventana | 0,986712 | 0,981956 | −0,48 % |
| Mandelbrot | 0,648574 | 0,653850 | +0,81 % |
| Cadenas | 1,354719 | 1,362005 | +0,54 % |
| Pi moderno | 2,085885 | 2,084412 | −0,07 % |
| Matrices | 0,977092 | 0,973492 | −0,37 % |

Jelly con ventana, 10.000 puntos y 1.600 fotogramas: **315,35 → 317,27 FPS**.
El tiempo externo mediano pasa de 5,100025 a 5,073944 segundos (−0,51 %).
Todos los tiempos internos empleados para calcular FPS son positivos y menores
que el tiempo externo.

## WSL

| Programa | 1.6.6 local elegida | 1.6.7 final | Cambio |
| --- | ---: | ---: | ---: |
| Pi, 10.000 dígitos | 2,233419 s | 2,235082 s | +0,07 % |
| Jelly sin ventana | 0,993743 s | 0,995366 s | +0,16 % |

No se midió Jelly con ventana en WSL. El cambio de compilador de Rust 1.95 a
1.98.1 se estudió anteriormente; ambos ejecutables de esta tabla ya usan 1.98.1.
No se confunde aquella mejora con una mejora nueva debida a cambiar la versión
de AVL-BASIC.

## Corrección y entrega

- `cargo fmt --all -- --check`: correcto.
- Rust Windows: 765 pruebas superadas, 0 fallidas, 7 ignoradas.
- Rust WSL: 775 superadas, 0 fallidas, 1 ignorada.
- Python de referencia: 1.185 superadas. Su repositorio permanece sin cambios
  funcionales, en 1.5.81; no se ha medido su rendimiento.
- Paridad explícita Windows/Python: 103 sesiones directas, 54 comprobaciones
  de framebuffer y 3 sesiones gráficas, todas correctas. Las integraciones Cargo
  opcionales mediante `AVL_BASIC_PY_REPO` no se activaron.
- Las secciones `.avlrun` y `.avleval` del EXE final pasan la comprobación PE:
  entradas alineadas a 4 KiB, permisos RX y metadatos de desenrollado válidos.
- Ambos binarios de desarrollo imprimen `1.6.7` mediante `PRINT VERSION$` y
  resuelven la comprobación directa `PRINT 6*7`.

Los paquetes se construyen con `--skip-build` a partir de estos mismos binarios.
Se auditan su contenido completo, las muestras, la documentación HTML sin
dependencias externas, los enlaces locales, las licencias y la correspondencia
entre archivo empaquetado y ejecutable de desarrollo. El manifiesto
`avl-basic-1.6.7-SHA256SUMS.txt` acompaña a los paquetes publicados.

| Binario final | SHA-256 |
| --- | --- |
| Windows | `8466c5df5fc816fd248be8f886a99e47ae3f60081c80b451c3032eb6acc9d979` |
| WSL/Linux | `7114cb642ae89a9c890199425e1edaf48827a54f1cba274668c6ad88cca00c83` |

El [JSON permanente](release-1.6.7-results.json) conserva tiempos individuales,
medianas, dispersión, hashes de corrección, manifiestos de compilación y hashes
de los registros de validación. La evidencia local completa está bajo
`target/release-sync-167/` y los manifiestos y binarios congelados bajo
`target/toolchain-comparison-20260925/release167-*`.
