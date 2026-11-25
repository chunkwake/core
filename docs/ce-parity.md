# CraftEngine parity: Paper vs Chunkwake

Один и тот же `CEProbe`/`CEStateProbe` и CraftEngine 26.9.2 на Paper 26.3
и Chunkwake `d095970`, чистые `plugins/CEProbe` (без старого `placed.flag`).
Сравниваются custom-блоки `default:topaz_ore`, `table_lamp`, `palm_stairs`,
`palm_door`: `BlockData`, state, collision, drops, persist/fresh. Исключены
отладочные dump-строки, identity hash, сгенерированный proxy-класс и
`place-first` (зависит от того, стоял ли блок в мире до прогона).

- Paper observations: **28**
- Chunkwake observations: **28**
- Mismatches: **0**

## Mismatches

Нет: все ключевые наблюдения совпали после нормализации.

## Совпавшие наблюдения

```text
CEProbe: CEPROBE done
CEProbe: CEPROBE drops=[NETHER_BRICKx1]
CEProbe: CEPROBE fresh at=1002,100,1000 custom=true type=BRICKS data=craftengine:custom_N state=default:topaz_ore
CEProbe: CEPROBE fresh collision-boxes=[BoundingBox [minX=0.0, minY=0.0, minZ=0.0, maxX=1.0, maxY=1.0, maxZ=1.0]]
CEProbe: CEPROBE fresh data-class=<BlockData> asString=craftengine:custom_N
CEProbe: CEPROBE persist at=1000,100,1000 custom=true type=BRICKS data=craftengine:custom_N state=default:topaz_ore
CEProbe: CEPROBE persist collision-boxes=[BoundingBox [minX=0.0, minY=0.0, minZ=0.0, maxX=1.0, maxY=1.0, maxZ=1.0]]
CEProbe: CEPROBE persist data-class=<BlockData> asString=craftengine:custom_N
CEProbe: CEPROBE place-second=true
CEStateProbe: CESTATEPROBE default:palm_door default=default:palm_door[facing=north,half=upper,hinge=left,open=false,powered=false] props={EnumProperty{clazz=class net.momirealms.craftengine.core.util.Direction, name='facing', values=[NORTH, EAST, SOUTH, WEST]}, EnumProperty{clazz=class net.momirealms.craftengine.core.block.property.type.DoubleBlockHalf, name='half', values=[UPPER, LOWER]}, EnumProperty{clazz=class net.momirealms.craftengine.core.block.property.type.DoorHinge, name='hinge', values=[LEFT, RIGHT]}, BooleanProperty{clazz=class java.lang.Boolean, name='open', values=[true, false]}, BooleanProperty{clazz=class java.lang.Boolean, name='powered', values=[true, false]}}
CEStateProbe: CESTATEPROBE default:palm_door desired=default:palm_door[facing=east,half=upper,hinge=right,open=true,powered=true]
CEStateProbe: CESTATEPROBE default:palm_door drops=[]
CEStateProbe: CESTATEPROBE default:palm_door fresh at=1032,100,1010 custom=true state=default:palm_door[facing=east,half=upper,hinge=right,open=true,powered=true] type=BRICKS data=craftengine:custom_N collision=[BoundingBox [minX=0.0, minY=0.0, minZ=0.8125, maxX=1.0, maxY=1.0, maxZ=1.0]]
CEStateProbe: CESTATEPROBE default:palm_door persist at=1030,100,1010 custom=true state=default:palm_door[facing=east,half=upper,hinge=right,open=true,powered=true] type=BRICKS data=craftengine:custom_N collision=[BoundingBox [minX=0.0, minY=0.0, minZ=0.8125, maxX=1.0, maxY=1.0, maxZ=1.0]]
CEStateProbe: CESTATEPROBE default:palm_door place-second=true
CEStateProbe: CESTATEPROBE default:palm_stairs default=default:palm_stairs[facing=north,half=bottom,shape=straight,waterlogged=false] props={EnumProperty{clazz=class net.momirealms.craftengine.core.util.Direction, name='facing', values=[NORTH, EAST, SOUTH, WEST]}, EnumProperty{clazz=class net.momirealms.craftengine.core.block.property.type.SingleBlockHalf, name='half', values=[BOTTOM, TOP]}, EnumProperty{clazz=class net.momirealms.craftengine.core.block.property.type.StairsShape, name='shape', values=[STRAIGHT, INNER_LEFT, INNER_RIGHT, OUTER_LEFT, OUTER_RIGHT]}, BooleanProperty{clazz=class java.lang.Boolean, name='waterlogged', values=[true, false]}}
CEStateProbe: CESTATEPROBE default:palm_stairs desired=default:palm_stairs[facing=east,half=top,shape=straight,waterlogged=true]
CEStateProbe: CESTATEPROBE default:palm_stairs drops=[NETHER_BRICKx1]
CEStateProbe: CESTATEPROBE default:palm_stairs fresh at=1022,100,1010 custom=true state=default:palm_stairs[facing=east,half=top,shape=straight,waterlogged=true] type=BRICKS data=craftengine:custom_N collision=[BoundingBox [minX=0.5, minY=0.0, minZ=0.0, maxX=1.0, maxY=1.0, maxZ=1.0], BoundingBox [minX=0.0, minY=0.5, minZ=0.0, maxX=0.5, maxY=1.0, maxZ=1.0]]
CEStateProbe: CESTATEPROBE default:palm_stairs persist at=1020,100,1010 custom=true state=default:palm_stairs[facing=east,half=top,shape=straight,waterlogged=true] type=BRICKS data=craftengine:custom_N collision=[BoundingBox [minX=0.5, minY=0.0, minZ=0.0, maxX=1.0, maxY=1.0, maxZ=1.0], BoundingBox [minX=0.0, minY=0.5, minZ=0.0, maxX=0.5, maxY=1.0, maxZ=1.0]]
CEStateProbe: CESTATEPROBE default:palm_stairs place-second=true
CEStateProbe: CESTATEPROBE default:table_lamp default=default:table_lamp[facing=north,lit=false] props={EnumProperty{clazz=class net.momirealms.craftengine.core.util.Direction, name='facing', values=[NORTH, EAST, SOUTH, WEST]}, BooleanProperty{clazz=class java.lang.Boolean, name='lit', values=[true, false]}}
CEStateProbe: CESTATEPROBE default:table_lamp desired=default:table_lamp[facing=east,lit=true]
CEStateProbe: CESTATEPROBE default:table_lamp drops=[NETHER_BRICKx1]
CEStateProbe: CESTATEPROBE default:table_lamp fresh at=1012,100,1010 custom=true state=default:table_lamp[facing=east,lit=true] type=BRICKS data=craftengine:custom_N collision=[BoundingBox [minX=0.0, minY=0.0, minZ=0.0, maxX=1.0, maxY=1.0, maxZ=1.0]]
CEStateProbe: CESTATEPROBE default:table_lamp persist at=1010,100,1010 custom=true state=default:table_lamp[facing=east,lit=true] type=BRICKS data=craftengine:custom_N collision=[BoundingBox [minX=0.0, minY=0.0, minZ=0.0, maxX=1.0, maxY=1.0, maxZ=1.0]]
CEStateProbe: CESTATEPROBE default:table_lamp place-second=true
CEStateProbe: CESTATEPROBE done
```

Сырые логи: `benchmarks/ce-parity/paper.txt` и
`benchmarks/ce-parity/chunkwake.txt`.

