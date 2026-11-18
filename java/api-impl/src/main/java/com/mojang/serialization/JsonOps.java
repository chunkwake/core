package com.mojang.serialization;

/** Минимальный DFU {@code JsonOps} (VoiceMessages/InvSee++). */
public final class JsonOps implements DynamicOps<Object> {
    public static final JsonOps INSTANCE = new JsonOps();

    private JsonOps() {
    }

    public com.google.gson.JsonElement empty() {
        return new com.google.gson.JsonObject();
    }
}
