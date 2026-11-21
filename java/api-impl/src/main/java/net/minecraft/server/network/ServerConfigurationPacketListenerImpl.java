package net.minecraft.server.network;

import java.util.Queue;
import java.util.concurrent.ConcurrentLinkedQueue;

/**
 * NMS {@code ServerConfigurationPacketListenerImpl}.
 *
 * <p>Emotecraft читает поле {@code configurationTasks} через VarHandle,
 * чтобы дождаться завершения configuration-задач.
 */
public class ServerConfigurationPacketListenerImpl extends ServerCommonPacketListenerImpl {
    public final Queue<Runnable> configurationTasks = new ConcurrentLinkedQueue<>();

    /** {@code returnToWorld()} (CraftEngine). */
    public void returnToWorld() {
    }

    public ServerConfigurationPacketListenerImpl() {
        super();
    }
}
