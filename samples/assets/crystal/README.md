# Crystal Siege structures

These CSV resources contain the constructions used by
[`g-crystal.bas`](../../g-crystal.bas):

| Construction | Resource | Nodes | Links | Panels |
|---|---|---:|---:|---:|
| Crystal Cathedral | [`cathedral.csv`](cathedral.csv) | 77 | 272 | 64 |
| Starfall Observatory | [`observatory.csv`](observatory.csv) | 73 | 272 | 64 |
| Skyport Terminal | [`skyport.csv`](skyport.csv) | 73 | 270 | 64 |

Starfall Observatory has eight splayed supports, an open central neck holding
the golden core, a broad octagonal crystal crown and a pointed faceted roof.
Its eight supports share the load; attacks from several sides progressively
unbalance the crown.

Skyport Terminal joins an offset control tower, a diagonal passenger gallery
on two piers, and a lower hangar with an unequal pitched roof and cantilever
canopy. Each body has its own supports. The order of attacks changes which
connections can still transfer loads as the structure falls. Its demo brings
down the tower and gallery while leaving the hangar for the remaining shots.

All three constructions allow ten shots and have their own six-shot
demonstrations. They use the same physics and solver settings.

The game reads the selected structure at startup or when choosing another
construction, with `OPEN ... FOR INPUT` and `INPUT #`. Rebuilds reuse the loaded
rest state. The CSV is a resource, not a BASIC program;
it has no line numbers or `DATA` statements.

The cathedral preserves its original numeric values, record order,
node/link/panel indices, impact behavior and demonstration. More constructions
can be added as data files.

## Choosing and adding constructions

Press **C**, use the arrow keys and press **Enter** to load a construction.
**Escape** returns to the current attempt. Confirming the construction already
in play also returns without restarting or recording a result. A successful
change ends the old attempt, offers to record a qualifying score for that
construction, and starts the selected one with its own top five.

`structures.csv` is the construction catalog. It begins with
`"CRYSTAL-CATALOG/1", count`, followed by exactly `count` records containing
`id, title, structure_file`. The current catalog is:

```csv
"CRYSTAL-CATALOG/1",3
"cathedral","Crystal Cathedral","assets/crystal/cathedral.csv"
"observatory","Starfall Observatory","assets/crystal/observatory.csv"
"skyport","Skyport Terminal","assets/crystal/skyport.csv"
```

The catalog supports 1 through 32 unique IDs. An ID uses 1 through 32 ASCII
letters, digits, underscores or hyphens and is case-sensitive. Keep it stable
when changing a construction's title: personal records are associated with the
ID. The structure file's header must contain the same ID as its catalog entry.
Paths are relative to the BASIC sample's directory. The first catalog entry is
loaded initially; the catalog is read when the game starts.

To add a construction, create a structure CSV and add a catalog row, adjusting
the catalog count. The game validates the new file before ending the active
attempt. If that file is invalid, the selector shows a short message and keeps
the previous construction and game state.

## CSV format, version 1

Records appear in the following order. Section names are case-sensitive. Each
record has exactly the listed fields; blank lines, comments and extra records
are not accepted. Decimal numbers use a point. Strings follow the same CSV rules
as BASIC `WRITE #` and `INPUT #`.

| Record or section | Fields |
|---|---|
| Header | `"CRYSTAL-STRUCTURE/1", id, title` |
| Counts | `"COUNTS", node_count, link_count, panel_count, core_node, shots` |
| Camera | `"CAMERA", yaw, pitch, distance, target_y, focus, view_y, minimum_distance, maximum_distance` |
| Nodes header | `"NODES", node_count` |
| Each node | `x, y, z, inverse_mass, kind, radius, color` |
| Links header | `"LINKS", link_count` |
| Each link | `node_a, node_b` |
| Panels header | `"PANELS", panel_count` |
| Each panel | `node_a, node_b, node_c, link_ab, link_bc, link_ca` |
| Bearings header | `"BEARINGS", bearing_count` |
| Each bearing | `node` |
| Demo header | `"DEMO", shot_count` |
| Each demo shot | `target_node, launch_x, launch_y, launch_z, camera_yaw, camera_pitch, camera_distance` |

All indices are zero-based; record order supplies the node, link and panel IDs.
An inverse mass of zero fixes a node in place. Colors index the game's seven-entry
palette, 0 through 6. Camera angles are radians; projection values use the game's
original logical 640-by-480 framing, independent of the selected display mode.
Link rest lengths are calculated by the game from the loaded node positions.

The bearings list supplies the node IDs used by the existing impact/fracture
logic. These IDs and the demo route belong to the structure. The cathedral keeps
its six original bearing IDs and six original demonstration shots.

The loader checks section tags, integer counts and indices, positive link lengths,
panel-to-link correspondence, duplicate bearings and camera limits. Current
capacities are 220 nodes, 500 links, 96 panels and six demo shots. A missing or
invalid initial catalog or construction closes the graphics window and reports
the problem in the console before the simulation starts. Optional construction
changes use a temporary copy of the rest data so a failed load can be rejected.

Keep the order of records and sufficient decimal precision when editing a model:
the sequential constraint solver can respond differently if links are reordered.
The current ground/support-map assumptions also favor structures of a similar
physical scale to the cathedral.

## Personal scores

Structure files contain geometry and starting parameters. All personal records
share `g-crystal-scores.csv` beside the BASIC sample, excluded from release
packages. Each construction has an independent top five. Records for IDs absent
from the current catalog are preserved too; the archive holds up to 64 IDs.

The shared archive has header `"CRYSTAL/2", record_count` and records of the form
`structure_id, name, score`. Original `CRYSTAL/1` files are read as cathedral
records. Loading a legacy archive does not rewrite it: the next accepted score
saves all groups in the new format. Loading a structure or rebuilding does not
write its resource file. A structure change only writes the score archive if
the old attempt produces an accepted qualifying entry.
