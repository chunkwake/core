package dev.chunkwake.bridge;

import java.util.Date;
import java.util.Objects;

/** Одна запись {@code banned-players.json}/{@code banned-ips.json}. */
final class BanEntryImpl implements org.bukkit.BanEntry {
    private final BanListImpl owner;
    final String target;
    private Date created;
    private String source;
    private Date expiration;
    private String reason;

    BanEntryImpl(BanListImpl owner, String target, Date created, String source,
            Date expiration, String reason) {
        this.owner = owner;
        this.target = target;
        this.created = created == null ? new Date() : created;
        this.source = source == null ? "Chunkwake" : source;
        this.expiration = expiration;
        this.reason = reason == null ? "Banned by an operator." : reason;
    }

    @Override public String getTarget() { return target; }

    @Override public Object getBanTarget() { return target; }

    @Override public Date getCreated() { return created; }

    @Override public void setCreated(Date created) {
        this.created = Objects.requireNonNull(created, "created");
    }

    @Override public String getSource() { return source; }

    @Override public void setSource(String source) {
        this.source = Objects.requireNonNull(source, "source");
    }

    @Override public Date getExpiration() { return expiration; }

    @Override public void setExpiration(Date expiration) { this.expiration = expiration; }

    @Override public String getReason() { return reason; }

    @Override public void setReason(String reason) {
        this.reason = Objects.requireNonNull(reason, "reason");
    }

    @Override public void save() { owner.save(); }

    @Override public void remove() { owner.pardon(target); }

    @Override public String toString() {
        return "BanEntry{" + target + ", expires=" + expiration + "}";
    }
}
