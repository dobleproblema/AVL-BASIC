# Seguimiento de rendimiento de AVL BASIC 1.6.0

Fecha: 21 de septiembre de 2026. Comparación en Windows frente al ejecutable
publicado de 1.5.96, siguiendo el compromiso de
[REGRESSION-2026-09-18.md](REGRESSION-2026-09-18.md).

| Ejecutable | SHA256 |
| --- | --- |
| 1.5.96 publicado | `87471d5e75f4d1049a1ed3fd5b31a5e8863c1e430043424f9fd00def3f7be1b5` |
| 1.6.0 para publicar | `1b44acee95c30acb1716e672987f4076533ef46529fa752df2925384e7c324aa` |

Se utilizó `representative.py` y la configuración fija `representative.json`:
Pi de 10.000 decimales y Jelly de 10.000 puntos durante 320 fotogramas.
Un calentamiento y seis repeticiones medidas por ejecutable/carga, con orden
alternado AB/BA y rotación de cargas. No hubo otras compilaciones, pruebas o
benchmarks del proyecto ejecutándose a la vez.

| Carga | Métrica | 1.5.96 | 1.6.0 | Aumento de tiempo |
| --- | --- | ---: | ---: | ---: |
| Pi 10.000 | Mediana externa, s | 1,64694 | 1,69076 | +2,66 % |
| Jelly sin ventana | Mediana externa, s | 0,92224 | 0,94555 | +2,53 % |
| Jelly con ventana real | Mediana del cuerpo, s | 0,97810 | 0,99551 | +1,78 % |

Las salidas, errores y PNG finales coinciden entre ambos ejecutables en todas
las ejecuciones. La comprobación del PNG no verifica todos los fotogramas
intermedios. Los programas no reproducen audio: esta comparación mide el
rendimiento general del intérprete con la nueva versión, no el coste de síntesis.

La distancia a 1.5.96 es menor que la observada en la instantánea del día 18
(+10,40 % en Pi y +7,18 % en Jelly sin ventana). Es una nueva comparación contra
su propia referencia; no permite atribuir la recuperación a un cambio aislado.
La regresión residual sigue abierta. La diferencia con ventana es pequeña
respecto a la dispersión y no debe interpretarse como un coste exacto universal.

Datos completos, incluidos medianas, dispersión, ejecuciones y hashes:

- [Sin ventana](release-1.6.0-headless-results.json)
- [Con ventana real](release-1.6.0-window-results.json)

Comandos de reproducción, desde la raíz del repositorio:

```powershell
python tools/benchmarks/representative.py --baseline release/avl-basic-1.5.96-windows-x64/avl-basic.exe --candidate target/release/avl-basic.exe --workloads pimachin,jelly --runs 6 --warmups 1 --output target/release-check-1.6.0/performance-headless
python tools/benchmarks/representative.py --baseline release/avl-basic-1.5.96-windows-x64/avl-basic.exe --candidate target/release/avl-basic.exe --workloads jelly --runs 6 --warmups 1 --window 1 --metric body_s --output target/release-check-1.6.0/performance-window
```

El directorio de salida debe ser nuevo en cada repetición de la comprobación.
