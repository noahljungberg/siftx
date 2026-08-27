package com.truespar.siftx;

import java.util.OptionalDouble;

/**
 * GPS coordinates in decimal degrees (WGS84).
 *
 * @param latitude  Decimal degrees, negative = south.
 * @param longitude Decimal degrees, negative = west.
 * @param altitude  Meters above sea level, or empty if unavailable.
 */
public record GpsCoordinates(double latitude, double longitude, OptionalDouble altitude) {
    @Override
    public String toString() {
        var sb = new StringBuilder();
        sb.append(String.format("%.6f, %.6f", latitude, longitude));
        altitude.ifPresent(a -> sb.append(String.format(", %.1fm", a)));
        return sb.toString();
    }
}
