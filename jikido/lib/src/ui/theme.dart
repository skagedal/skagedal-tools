import 'package:flutter/material.dart';

/// The palette: sumi ink, unbleached paper, and the vermilion of a temple
/// pillar. Dark throughout — a timer you start and then stop looking at
/// should not light up the room.
abstract final class JikidoColors {
  static const Color ink = Color(0xFF14110F);
  static const Color inkRaised = Color(0xFF1E1A17);
  static const Color paper = Color(0xFFE9E2D5);
  static const Color faded = Color(0xFF8C8377);
  static const Color vermilion = Color(0xFFC65D3B);
}

ThemeData jikidoTheme() {
  const colorScheme = ColorScheme.dark(
    primary: JikidoColors.vermilion,
    onPrimary: JikidoColors.ink,
    secondary: JikidoColors.faded,
    onSecondary: JikidoColors.ink,
    surface: JikidoColors.ink,
    onSurface: JikidoColors.paper,
    surfaceContainerHighest: JikidoColors.inkRaised,
    outline: JikidoColors.faded,
  );

  final base = ThemeData.from(colorScheme: colorScheme, useMaterial3: true);

  return base.copyWith(
    scaffoldBackgroundColor: JikidoColors.ink,
    appBarTheme: const AppBarTheme(
      backgroundColor: JikidoColors.ink,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      centerTitle: true,
    ),
    textTheme: base.textTheme.apply(
      bodyColor: JikidoColors.paper,
      displayColor: JikidoColors.paper,
    ),
    dividerTheme: const DividerThemeData(color: Color(0x22E9E2D5), space: 1),
    listTileTheme: const ListTileThemeData(
      textColor: JikidoColors.paper,
      iconColor: JikidoColors.faded,
    ),
  );
}
