package org.json.simple.parser;

/** json-simple `ContainerFactory`: чем создавать объекты/массивы при разборе. */
public interface ContainerFactory {
    Object createObjectContainer();

    Object createArrayContainer();
}
