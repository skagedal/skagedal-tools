import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_foreground_task/flutter_foreground_task.dart';

import 'src/sitting_controller.dart';
import 'src/ui/sitting_page.dart';
import 'src/ui/theme.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  FlutterForegroundTask.initCommunicationPort();
  SystemChrome.setPreferredOrientations([DeviceOrientation.portraitUp]);
  runApp(const JikidoApp());
}

class JikidoApp extends StatefulWidget {
  const JikidoApp({super.key});

  @override
  State<JikidoApp> createState() => _JikidoAppState();
}

class _JikidoAppState extends State<JikidoApp> {
  final SittingController _controller = SittingController();

  @override
  void initState() {
    super.initState();
    // Deliberately not awaited before the first frame: the UI is usable with
    // the default settings, and loading audio behind a splash screen only
    // makes the app feel slower.
    _controller.initialize();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => MaterialApp(
        title: 'Jikido',
        debugShowCheckedModeBanner: false,
        theme: jikidoTheme(),
        home: SittingPage(controller: _controller),
      );
}
