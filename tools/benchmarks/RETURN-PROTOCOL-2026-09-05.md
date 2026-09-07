# Comprobación estática del protocolo de retorno — 2026-09-05

Inspección de los binarios existentes mediante `nm`, `readelf -wf` y `objdump -d`. No se compilaron ni ejecutaron programas, no se tomaron perfiles nuevos y no se modificaron binarios para preparar esta nota.

Los candidatos analizados fueron descartados y sus parches experimentales se han retirado. Las mediciones y el análisis histórico de esta nota se mantienen sin cambios.

## Alcance y atribución

La referencia `wsl-names-build/release/avl-basic` conserva el símbolo `avl_basic::interpreter::FastNumberExpr::eval`. Los ejecutables finales de los dos candidatos están despojados de símbolos. Sus `.rlib` contienen bitcode LLVM; el `nm` instalado no puede leer ese formato y no se dispone de `llvm-dis`/`llvm-nm`.

En los candidatos se localizaron rangos de función mediante las entradas FDE de `.eh_frame`. La atribución al evaluador se infiere de la combinación de doce llamadas directas recursivas al comienzo del mismo rango, selección del enum `FastNumberExpr` con el mismo patrón de discriminantes que la referencia y ramas correspondientes al número literal, arrays y operadores. Las instrucciones citadas y los límites FDE están observados; el nombre `eval_inner` del candidato es esa atribución por estructura, no un nombre recuperado de símbolos. Esta nota no presenta una reconstrucción general del binario.

| Variante | Rango identificado | Tamaño del rango | Reserva local del prólogo | Registros salvados | Retorno observado |
|---|---|---:|---:|---:|---|
| Referencia | `0x117a00..0x118e68` | 5.224 bytes | `0x98` = 152 bytes | 6 | Buffer de memoria |
| Error con Box | `0x119240..0x11a9d9` | 6.041 bytes | `0x118` = 280 bytes | 6 | Buffer de memoria |
| Error compartido | `0x118d00..0x11a30a` | 5.642 bytes | `0x108` = 264 bytes | 6 | `f64` en `xmm0`, estado de error separado |

La reserva local excluye los seis registros salvados y la dirección de retorno. Los tres rangos contienen doce sitios de llamada recursiva. El tamaño de código y de pila no cuantifica por sí solo su coste temporal ni demuestra una causa de las regresiones.

## Instrucciones que permiten contrastar la hipótesis

**Referencia.** La rama de un número literal escribe el `f64` en `[rbx+8]` (`0x117a53`) y el discriminante en `[rbx]` (`0x117a58`), siendo `rbx` el buffer de retorno recibido en `rdi`. Una llamada recursiva prepara `rdi=rsp+0x28` en `0x117ac7`, llama a `0x117a00` en `0x117acf`, vuelve a leer el discriminante y el valor desde la pila y comprueba el error. En la rama de propagación de error `0x117fca..0x117fdf` se copian 40 bytes de resultado. **La rama correcta no copia esos 40 bytes**: escribe discriminante y número.

**Box.** La llamada recursiva de `0x119298..0x1192a0` vuelve a pasar `rdi=rsp+0x28`. Después compara el discriminante en `[rsp+0x28]` y carga el número de `[rsp+0x30]`. La salida correcta de `0x119c91` escribe `xmm0` en `[rbx+8]` y en `0x119c96` escribe cero en `[rbx]`. La representación más pequeña del error **no eliminó el buffer de retorno en esta compilación Linux**. No se debe describir este candidato como una medición exitosa del beneficio de devolver el número mediante un registro.

**Estado compartido.** La llamada de `0x118d58..0x118d62` pasa el hijo en `rdi`, el intérprete en `rsi` y el mismo estado de error en `rdx`; no prepara un buffer de retorno para el número. Al regresar comprueba el estado en `[rbx]` y usa el número de `xmm0`. El epílogo `0x119cfa..0x119d0f` coloca el valor en `xmm0`, restaura la pila y retorna. Por tanto, esta variante sí modifica el protocolo de retorno pretendido, pero conserva la recursión, el despacho por nodo y comprobaciones del estado de error después de cada hijo. Conseguir el cambio de protocolo no prueba que el balance completo de instrucciones y dependencias sea más rápido.

## Identidad de los candidatos inspeccionados

- `target/perf-20260905/compact-error-wsl/release/avl-basic`: SHA-256 `382243a78c1dc21b0916127e59cd2441234e60ddf7656227ef526844feb43acf`.
- `target/perf-20260905/shared-error-wsl/release/avl-basic`: SHA-256 `b6027de2b3771fca11472ed33aa82c1f925224c884f391a81640655fd5e3810b`.

No se extrapola este análisis del ensamblador Linux al protocolo exacto generado en Windows. Los resultados A/B de cada plataforma son los que determinan si interesa conservar el cambio. Estos hallazgos no atribuyen una regresión concreta a LLVM, cachés, predicción de saltos o presión de registros; faltaría evidencia adicional para esa atribución y no hace falta obtenerla para descartar candidatos que empeoran las cargas medidas.
