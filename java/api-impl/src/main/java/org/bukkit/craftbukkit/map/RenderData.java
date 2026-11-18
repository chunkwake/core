package org.bukkit.craftbukkit.map;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.map.MapCursor;

/** CraftBukkit {@code RenderData}. */
public class RenderData {
    public byte[] buffer = new byte[0];
    public List<MapCursor> cursors = new ArrayList<>();
}
